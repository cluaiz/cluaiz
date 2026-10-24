// C ABI only: native tool calls through llama.cpp's common chat layer. The template renders the
// tools and tool turns in the model's own format, common_chat_templates_apply gives the lazy
// tool-call grammar, and common_chat_parse reads the output back. No format logic lives here.
#include "chat.h"
#include "common.h"
#include "json-schema-to-grammar.h"
#include "llama.h"

#include <cstdlib>
#include <cstring>
#include <memory>
#include <string>
#include <vector>

namespace {

char * dup_string(const std::string & text) {
    char * out = static_cast<char *>(std::malloc(text.size() + 1));
    if (out) {
        std::memcpy(out, text.data(), text.size());
        out[text.size()] = '\0';
    }
    return out;
}

char * error_json(const char * message) {
    auto out = common_json::object();
    out["error"] = std::string(message ? message : "unknown error");
    return dup_string(out.dump_safe());
}

const char * trigger_type_name(common_grammar_trigger_type type) {
    switch (type) {
        case COMMON_GRAMMAR_TRIGGER_TYPE_TOKEN:        return "token";
        case COMMON_GRAMMAR_TRIGGER_TYPE_WORD:         return "word";
        case COMMON_GRAMMAR_TRIGGER_TYPE_PATTERN:      return "pattern";
        case COMMON_GRAMMAR_TRIGGER_TYPE_PATTERN_FULL: return "pattern_full";
    }
    return "word";
}

}  // namespace

struct cluaiz_chat_templates {
    common_chat_templates_ptr tmpls;
};

struct cluaiz_chat_parser {
    common_chat_parser_params params;
};

extern "C" cluaiz_chat_templates * cluaiz_chat_templates_init(const llama_model * model, const char * custom_tmpl) noexcept {
    try {
        auto tmpls = common_chat_templates_init(model, custom_tmpl ? custom_tmpl : "");
        return tmpls ? new cluaiz_chat_templates{ std::move(tmpls) } : nullptr;
    } catch (...) {
        return nullptr;
    }
}

extern "C" void cluaiz_chat_templates_free(cluaiz_chat_templates * tmpls) noexcept {
    delete tmpls;
}

// request: {"messages": [OpenAI messages], "tools": [OpenAI tools], "enable_thinking": bool,
//           "chat_template_kwargs": {..}}
// returns: {"prompt", "format", "grammar", "grammar_lazy", "triggers": [{"type", "value", "token"}],
//           "preserved_tokens", "additional_stops", "generation_prompt", "parser"} or {"error"}
extern "C" char * cluaiz_chat_apply(const cluaiz_chat_templates * tmpls, const char * request) noexcept {
    if (!tmpls || !tmpls->tmpls || !request) {
        return error_json("no chat templates");
    }
    try {
        const auto body = common_json::parse(request);
        common_chat_templates_inputs inputs;
        inputs.messages = common_chat_msgs_parse_oaicompat(body.at("messages"));
        if (body.contains("tools")) {
            inputs.tools = common_chat_tools_parse_oaicompat(body.at("tools"));
        }
        inputs.tool_choice         = COMMON_CHAT_TOOL_CHOICE_AUTO;
        // The Engine runs one call per step and renders the conversation again after it
        inputs.parallel_tool_calls = false;
        inputs.reasoning_format    = COMMON_REASONING_FORMAT_DEEPSEEK;
        inputs.enable_thinking     = body.value("enable_thinking", true);
        inputs.add_generation_prompt = true;
        if (body.contains("chat_template_kwargs") && body.at("chat_template_kwargs").is_object()) {
            for (const auto & [key, value] : body.at("chat_template_kwargs").items()) {
                inputs.chat_template_kwargs[key] = value.dump();
            }
        }
        const auto params = common_chat_templates_apply(tmpls->tmpls.get(), inputs);

        auto out = common_json::object();
        out["prompt"]            = params.prompt;
        // A template that never renders tools gets them inline instead
        const auto caps          = common_chat_templates_get_caps(tmpls->tmpls.get());
        const auto tools_cap     = caps.find("supports_tools");
        out["supports_tools"]    = tools_cap != caps.end() && tools_cap->second;
        out["format"]            = std::string(common_chat_format_name(params.format));
        out["format_id"]         = static_cast<int>(params.format);
        out["grammar"]           = params.grammar;
        out["grammar_lazy"]      = params.grammar_lazy;
        out["generation_prompt"] = params.generation_prompt;
        out["parser"]            = params.parser;
        auto triggers = common_json::array();
        for (const auto & trigger : params.grammar_triggers) {
            auto item = common_json::object();
            item["type"]  = std::string(trigger_type_name(trigger.type));
            item["value"] = trigger.value;
            item["token"] = static_cast<int>(trigger.token);
            triggers.push_back(item);
        }
        out["triggers"] = triggers;
        auto preserved = common_json::array();
        for (const auto & token : params.preserved_tokens) {
            preserved.push_back(token);
        }
        out["preserved_tokens"] = preserved;
        auto stops = common_json::array();
        for (const auto & stop : params.additional_stops) {
            stops.push_back(stop);
        }
        out["additional_stops"] = stops;
        return dup_string(out.dump_safe());
    } catch (const std::exception & e) {
        return error_json(e.what());
    } catch (...) {
        return error_json("chat template failed");
    }
}

// apply: the JSON cluaiz_chat_apply returned (format_id, generation_prompt, parser)
extern "C" cluaiz_chat_parser * cluaiz_chat_parser_init(const char * apply) noexcept {
    try {
        const auto body = common_json::parse(apply);
        auto parser = std::make_unique<cluaiz_chat_parser>();
        parser->params.format            = static_cast<common_chat_format>(body.value("format_id", 0));
        parser->params.reasoning_format  = COMMON_REASONING_FORMAT_DEEPSEEK;
        parser->params.generation_prompt = body.value("generation_prompt", std::string());
        parser->params.parse_tool_calls  = true;
        const std::string definition = body.value("parser", std::string());
        if (!definition.empty()) {
            parser->params.parser.load(definition);
        }
        return parser.release();
    } catch (...) {
        return nullptr;
    }
}

extern "C" void cluaiz_chat_parser_free(cluaiz_chat_parser * parser) noexcept {
    delete parser;
}

// The model's output so far (everything it generated, thinking included): {"content",
// "reasoning_content", "tool_calls": [{"name", "arguments", "id"}]} or {"error"}
extern "C" char * cluaiz_chat_parse(const cluaiz_chat_parser * parser, const char * text, bool partial) noexcept {
    if (!parser || !text) {
        return error_json("no parser");
    }
    try {
        const auto msg = common_chat_parse(text, partial, parser->params);
        auto out = common_json::object();
        out["content"]           = msg.content;
        out["reasoning_content"] = msg.reasoning_content;
        auto calls = common_json::array();
        for (const auto & call : msg.tool_calls) {
            auto item = common_json::object();
            item["name"]      = call.name;
            item["arguments"] = call.arguments;
            item["id"]        = call.id;
            calls.push_back(item);
        }
        out["tool_calls"] = calls;
        return dup_string(out.dump_safe());
    } catch (const std::exception & e) {
        return error_json(e.what());
    } catch (...) {
        return error_json("parse failed");
    }
}

// A JSON schema as a GBNF grammar (llama-server's response_format): {"grammar"} or {"error"}
extern "C" char * cluaiz_json_schema_grammar(const char * schema) noexcept {
    try {
        auto out = common_json::object();
        out["grammar"] = json_schema_to_grammar(common_json::parse(schema ? schema : "{}"));
        return dup_string(out.dump_safe());
    } catch (const std::exception & e) {
        return error_json(e.what());
    } catch (...) {
        return error_json("schema not supported");
    }
}

extern "C" void cluaiz_chat_string_free(char * text) noexcept {
    std::free(text);
}

// ── Tool-call grammar that sleeps while the model thinks ─────────────────────────────────────
// llama-server applies a lazy tool grammar only outside the reasoning block and replays the end
// tag into it when thinking ends (common_sampler_accept). The generation loop knows when the
// model thinks, so it switches this wrapper; the grammar itself is llama.cpp's.

struct cluaiz_tool_grammar {
    llama_sampler * grammar;
    bool            active;
};

static const char * tool_grammar_name(const llama_sampler * /*smpl*/) {
    return "cluaiz-tool-grammar";
}

static void tool_grammar_accept(llama_sampler * smpl, llama_token token) {
    auto * ctx = static_cast<cluaiz_tool_grammar *>(smpl->ctx);
    if (ctx->active) {
        llama_sampler_accept(ctx->grammar, token);
    }
}

static void tool_grammar_apply(llama_sampler * smpl, llama_token_data_array * cur_p) {
    auto * ctx = static_cast<cluaiz_tool_grammar *>(smpl->ctx);
    if (ctx->active) {
        llama_sampler_apply(ctx->grammar, cur_p);
    }
}

static void tool_grammar_reset(llama_sampler * smpl) {
    auto * ctx = static_cast<cluaiz_tool_grammar *>(smpl->ctx);
    llama_sampler_reset(ctx->grammar);
}

static void tool_grammar_free(llama_sampler * smpl) {
    auto * ctx = static_cast<cluaiz_tool_grammar *>(smpl->ctx);
    llama_sampler_free(ctx->grammar);
    delete ctx;
}

static llama_sampler * tool_grammar_clone(const llama_sampler * smpl);

// CPU only: the backend sampling members stay null
static llama_sampler_i make_tool_grammar_iface() {
    llama_sampler_i iface{};
    iface.name   = tool_grammar_name;
    iface.accept = tool_grammar_accept;
    iface.apply  = tool_grammar_apply;
    iface.reset  = tool_grammar_reset;
    iface.clone  = tool_grammar_clone;
    iface.free   = tool_grammar_free;
    return iface;
}

static llama_sampler_i tool_grammar_iface = make_tool_grammar_iface();

static llama_sampler * tool_grammar_clone(const llama_sampler * smpl) {
    const auto * ctx = static_cast<const cluaiz_tool_grammar *>(smpl->ctx);
    return llama_sampler_init(&tool_grammar_iface, new cluaiz_tool_grammar{ llama_sampler_clone(ctx->grammar), ctx->active });
}

// apply: the JSON cluaiz_chat_apply returned. Null when it has no grammar or the grammar fails.
// Starts inactive: prompt tokens the chain accepts never reach the grammar.
extern "C" llama_sampler * cluaiz_tool_grammar_init(const llama_vocab * vocab, const char * apply) noexcept {
    try {
        const auto body = common_json::parse(apply);
        const std::string grammar = body.value("grammar", std::string());
        if (grammar.empty()) {
            return nullptr;
        }
        // Same trigger handling as common_sampler_init (and llama-server's single-token words)
        std::vector<std::string> patterns;
        std::vector<llama_token> tokens;
        for (const auto & trigger : body.at("triggers")) {
            const std::string type  = trigger.value("type", std::string("word"));
            const std::string value = trigger.value("value", std::string());
            if (type == "token") {
                tokens.push_back(static_cast<llama_token>(trigger.value("token", -1)));
            } else if (type == "word") {
                const auto ids = common_tokenize(vocab, value, false, true);
                if (ids.size() == 1) {
                    tokens.push_back(ids[0]);
                } else {
                    patterns.push_back(regex_escape(value));
                }
            } else if (type == "pattern") {
                patterns.push_back(value);
            } else {
                std::string anchored = "^$";
                if (!value.empty()) {
                    anchored = (value.front() != '^' ? "^" : "") + value + (value.back() != '$' ? "$" : "");
                }
                patterns.push_back(anchored);
            }
        }
        std::vector<const char *> patterns_c;
        for (const auto & pattern : patterns) {
            patterns_c.push_back(pattern.c_str());
        }
        llama_sampler * inner = body.value("grammar_lazy", false)
            ? llama_sampler_init_grammar_lazy_patterns(vocab, grammar.c_str(), "root", patterns_c.data(),
                  patterns_c.size(), tokens.data(), tokens.size())
            : llama_sampler_init_grammar(vocab, grammar.c_str(), "root");
        if (!inner) {
            return nullptr;
        }
        return llama_sampler_init(&tool_grammar_iface, new cluaiz_tool_grammar{ inner, false });
    } catch (...) {
        return nullptr;
    }
}

// Switches the grammar on (outside thinking) or off. `replay`: text fed to the grammar when it
// switches on, the thinking end tag as llama-server replays it; may be empty.
extern "C" void cluaiz_tool_grammar_set_active(llama_sampler * smpl, const llama_vocab * vocab, bool active, const char * replay) noexcept {
    if (!smpl) {
        return;
    }
    auto * ctx = static_cast<cluaiz_tool_grammar *>(smpl->ctx);
    if (active && !ctx->active && replay && replay[0]) {
        try {
            for (const auto token : common_tokenize(vocab, replay, false, true)) {
                llama_sampler_accept(ctx->grammar, token);
            }
        } catch (...) {
            // the end tag is not part of the grammar's language: it only matters for triggers
        }
    }
    ctx->active = active;
}
