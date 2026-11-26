//! One agent turn, shared by every chat front end.
//!
//! The model answers; when the answer ends with tool calls, each call runs and its result joins the
//! conversation, and the model answers again. The turn ends with an answer that makes no call, a call
//! the client runs itself (OpenAI `tool_calls`), a stop (Ctrl+C, a cancelled stream, a closed
//! client), or the step limit. At the step or repeat limit a front end can ask for one last answer
//! without tools ([`Agent::prepare_final_answer`]), which [`run`] then generates.
//!
//! The API chat (streaming and not) and the terminal agent run their turns through [`run`]. Each
//! brings what differs through [`Agent`]: how the model is asked (a prompt continuation or a
//! message history), which tools exist and how a call is approved, and where the user sees it (SSE
//! chunks or the terminal). The order of the steps, the step limit and what a stop means are
//! decided here once.
//!
//! An answer that cannot be used as it is (a tool call that cannot be read, or nothing but
//! thinking) is not run and does not end the turn at once: the model is told what was wrong and
//! answers again, at most [`MOST_CORRECTIONS`] times in a row.

use std::future::Future;

/// A call as the model wrote it
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCall {
    /// The function name the model used
    pub name: String,
    /// Its arguments (JSON text)
    pub arguments: String,
    /// The call's text exactly as generated, for front ends that keep the model's own form
    pub raw: String,
}

/// What one generation produced
#[derive(Debug)]
pub enum Generated {
    /// `said`: the text before the calls (the user saw it). `calls`: the calls at its end, in order
    Answer { said: String, calls: Vec<ToolCall> },
    /// The user or the client stopped the generation; `said` is what was written before that
    Stopped { said: String },
    /// Nothing in it can be run or shown as the answer; `said` is the text before the problem
    Unusable { said: String, problem: Unusable },
}

/// Why an answer could not be used as it is
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unusable {
    /// A tool call that could not be read (half a block, arguments that are not a JSON object,
    /// markup the model's parser did not take); the text is the call as written
    UnreadableCall(String),
    /// No answer and no call: the model wrote only thinking, or nothing
    NoAnswer,
}

/// Longest part of an unreadable call quoted back to the model
const QUOTED_CHARS: usize = 300;

impl Unusable {
    /// What the model is told before it answers again (managed prompts `agent.call_unreadable`,
    /// `agent.no_answer`)
    pub fn note(&self) -> String {
        match self {
            Unusable::UnreadableCall(text) => {
                let text = text.trim();
                let mut quoted: String = text.chars().take(QUOTED_CHARS).collect();
                if text.chars().count() > QUOTED_CHARS {
                    quoted.push_str(" ...");
                }
                cluaiz_core::prompts::render("agent.call_unreadable", &[("call", &quoted)])
            }
            Unusable::NoAnswer => cluaiz_core::prompts::text("agent.no_answer"),
        }
    }
}

/// What running one call came to
#[derive(Debug)]
pub enum CallRun {
    /// The result the model gets (an error or a denial is a result too: the model reads it)
    Done(String),
    /// The client runs this function itself: the turn ends with the call
    Client,
    /// The user stopped the turn while the call ran or waited for approval
    Stop,
}

/// How a turn ended
#[derive(Debug, PartialEq, Eq)]
pub enum Ended {
    /// An answer without calls
    Answered(String),
    /// A call the client runs itself; `said` came before it
    ClientCall { said: String, call: ToolCall },
    /// Stopped; `said` is the text of the step that was stopped
    Stopped(String),
    /// `max_steps` generations made calls; the last results are in the conversation
    StepLimit,
    /// The model's answer could not be used, also after [`MOST_CORRECTIONS`] corrections (or the
    /// front end cannot add a correction); `said` is the last answer's text before the problem
    Unusable { said: String, problem: Unusable },
}

/// A front end's part of a turn. Futures are `Send` so an API turn can run in its own task.
pub trait Agent: Send {
    /// Generates the next answer. `step` 0 is the first; later steps follow the results recorded
    /// with [`Agent::record_result`]. The front end shows the text as it is generated.
    fn generate(&mut self, step: usize) -> impl Future<Output = anyhow::Result<Generated>> + Send;

    /// Runs (or hands over, or refuses) one call; approval happens here
    fn call(&mut self, call: &ToolCall) -> impl Future<Output = CallRun> + Send;

    /// The answer of a step that makes calls joins the conversation, before its results
    fn record_answer(&mut self, said: &str, calls: &[ToolCall]);

    /// A call's result joins the conversation
    fn record_result(&mut self, call: &ToolCall, result: &str);

    /// An unusable answer (its text before the problem) and what the model is told about it join
    /// the conversation, so the next generation can do better. False: this front end cannot add
    /// them, and the turn ends with [`Ended::Unusable`].
    fn record_correction(&mut self, said: &str, note: &str) -> bool;

    /// The user or the client stopped the turn (checked before each generation and call)
    fn stopped(&self) -> bool;

    /// The turn reached the step or repeat limit: true readies one more generation, without tools,
    /// that answers from the tool results so far (the managed prompt `agent.final_answer`), and
    /// [`run`] generates it. Small models with a long prompt kept calling tools until the limit and
    /// the turn had no answer (found in a soak on 2026-11-23). False (the default): the turn ends
    /// with [`Ended::StepLimit`], e.g. when the client runs its own functions.
    fn prepare_final_answer(&mut self) -> bool {
        false
    }
}

/// Identical calls (same name, same arguments) since the last new call: the second one is not run,
/// its result is the first one's again with the managed prompt `agent.call_repeated`
pub const REPEAT_REUSE_AT: usize = 2;
/// Identical calls since the last new call that end the steps as stuck (`Ended::StepLimit`); the
/// front end can then ask for a final answer without tools ([`Agent::prepare_final_answer`])
pub const REPEAT_STOP_AT: usize = 3;
/// Unusable answers in a row the model is asked to correct; the next one ends the turn
pub const MOST_CORRECTIONS: usize = 2;

/// A call's identity for repeat detection: the name (trimmed, lower case) and the arguments as
/// canonical JSON (object keys sorted, no whitespace), so `{"b":1, "a":2}` and `{"a":2,"b":1}` are
/// the same call. Arguments that are not JSON compare as their trimmed text.
fn call_signature(call: &ToolCall) -> (String, String) {
    let canonical_args = serde_json::from_str::<serde_json::Value>(&call.arguments)
        .ok()
        .and_then(|v| serde_json::to_string(&sorted_keys(v)).ok())
        .unwrap_or_else(|| call.arguments.trim().to_owned());
    (call.name.trim().to_ascii_lowercase(), canonical_args)
}

/// The value with every object's keys in sorted order, whatever map type serde_json was built with
fn sorted_keys(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut pairs: Vec<(String, serde_json::Value)> = map.into_iter().collect();
            pairs.sort_by(|a, b| a.0.cmp(&b.0));
            serde_json::Value::Object(pairs.into_iter().map(|(k, v)| (k, sorted_keys(v))).collect())
        }
        serde_json::Value::Array(items) => serde_json::Value::Array(items.into_iter().map(sorted_keys).collect()),
        other => other,
    }
}

/// What the repeat guard decides for one call
#[derive(Debug, PartialEq, Eq)]
enum Repeat {
    /// Run the call
    Run,
    /// Do not run it: the earlier identical call's result
    Reuse(String),
    /// Do not run it: the turn is stuck; the earlier result (if any) goes with the stop
    Stop(Option<String>),
}

/// Repeated identical calls within one turn. Counts start again after every call the turn had not
/// seen before (progress: a new call may change what an earlier one returns, e.g. a file read after
/// an edit), so a coding agent's read, edit, read again still runs the second read. Calls the model
/// only alternates between are caught: in run 77 Llama 3.2 1B called `read_item` with `celsius`
/// and `fahrenheit` in turn for 100 steps, and the old back-to-back count never got past 2.
#[derive(Default)]
struct Repeats {
    /// Every call signature seen in this turn
    seen: std::collections::HashSet<(String, String)>,
    /// Since the last new call: how often each signature came, and its last result
    since_new: std::collections::HashMap<(String, String), (usize, Option<String>)>,
}

impl Repeats {
    fn check(&mut self, sig: &(String, String)) -> Repeat {
        if self.seen.insert(sig.clone()) {
            self.since_new.clear();
        }
        let entry = self.since_new.entry(sig.clone()).or_insert((0, None));
        entry.0 += 1;
        match (entry.0, &entry.1) {
            (n, earlier) if n >= REPEAT_STOP_AT => Repeat::Stop(earlier.clone()),
            (n, Some(earlier)) if n >= REPEAT_REUSE_AT => Repeat::Reuse(earlier.clone()),
            _ => Repeat::Run,
        }
    }

    /// The result of a call that ran, for an identical call that may follow
    fn ran(&mut self, sig: &(String, String), result: &str) {
        if let Some(entry) = self.since_new.get_mut(sig) {
            entry.1 = Some(result.to_owned());
        }
    }
}

/// The result of a call that is not run because it repeats an earlier one (`agent.call_repeated`)
fn repeated_note(name: &str, earlier: &str) -> String {
    cluaiz_core::prompts::render("agent.call_repeated", &[("name", name), ("result", earlier)])
}

/// Runs one turn: at most `max_steps` generations (`0` = unlimited, guarded by repeat detection).
/// At the step or repeat limit, one more generation without tools when the front end readies it
/// ([`Agent::prepare_final_answer`]): an answer without calls is [`Ended::Answered`]; one that still
/// makes a call (not run) or cannot be used stays [`Ended::StepLimit`].
pub async fn run<A: Agent>(agent: &mut A, max_steps: usize) -> anyhow::Result<Ended> {
    let ended = steps(agent, max_steps).await?;
    if ended != Ended::StepLimit || agent.stopped() || !agent.prepare_final_answer() {
        return Ok(ended);
    }
    let ended = match agent.generate(max_steps).await? {
        Generated::Answer { said, calls } if calls.is_empty() => Ended::Answered(said),
        Generated::Stopped { said } => Ended::Stopped(said),
        Generated::Answer { .. } | Generated::Unusable { .. } => Ended::StepLimit,
    };
    if ended == Ended::StepLimit {
        tracing::info!("🔁 [Turn] The final answer without tools still made a call (or could not be used); only the text before it is the answer");
    }
    Ok(ended)
}

/// The steps of a turn, up to its ending or the step or repeat limit
async fn steps<A: Agent>(agent: &mut A, max_steps: usize) -> anyhow::Result<Ended> {
    let limit = if max_steps == 0 { usize::MAX } else { max_steps };
    let mut repeats = Repeats::default();
    let mut corrections: usize = 0;

    for step in 0..limit {
        if agent.stopped() {
            return Ok(Ended::Stopped(String::new()));
        }
        let (said, calls) = match agent.generate(step).await? {
            Generated::Stopped { said } => return Ok(Ended::Stopped(said)),
            Generated::Unusable { said, problem } => {
                corrections += 1;
                if corrections > MOST_CORRECTIONS || !agent.record_correction(&said, &problem.note()) {
                    return Ok(Ended::Unusable { said, problem });
                }
                continue;
            }
            Generated::Answer { said, calls } if calls.is_empty() => return Ok(Ended::Answered(said)),
            Generated::Answer { said, calls } => (said, calls),
        };
        corrections = 0;
        agent.record_answer(&said, &calls);
        for call in &calls {
            if agent.stopped() {
                return Ok(Ended::Stopped(said));
            }
            let sig = call_signature(call);
            match repeats.check(&sig) {
                Repeat::Run => {}
                Repeat::Reuse(earlier) => {
                    tracing::info!("🔁 [Turn] Step {step}: '{}' repeats an earlier call with the same arguments; not run again, its earlier result is returned", call.name);
                    agent.record_result(call, &repeated_note(&call.name, &earlier));
                    continue;
                }
                Repeat::Stop(earlier) => {
                    // The call gets a result too, so the conversation the final answer reads has
                    // no call without a result
                    tracing::info!("🛑 [Turn] Step {step}: '{}' with the same arguments came {REPEAT_STOP_AT} times; tool steps stop here", call.name);
                    agent.record_result(call, &repeated_note(&call.name, earlier.as_deref().unwrap_or_default()));
                    return Ok(Ended::StepLimit);
                }
            }
            match agent.call(call).await {
                CallRun::Done(result) => {
                    repeats.ran(&sig, &result);
                    agent.record_result(call, &result);
                }
                CallRun::Client => return Ok(Ended::ClientCall { said, call: call.clone() }),
                CallRun::Stop => return Ok(Ended::Stopped(said)),
            }
        }
    }
    Ok(Ended::StepLimit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// Scripted answers; records what the core asked for
    struct Script {
        answers: VecDeque<Generated>,
        client: Vec<&'static str>,
        stop_on_call: Option<&'static str>,
        log: Vec<String>,
        stopped: bool,
        /// Whether corrections can be added to the conversation
        corrects: bool,
        /// Whether a final answer without tools is readied at the step or repeat limit
        final_answer: bool,
    }

    fn call(name: &str) -> ToolCall {
        ToolCall { name: name.into(), arguments: "{}".into(), raw: format!("<call {name}>") }
    }

    fn answer(said: &str, calls: &[&str]) -> Generated {
        Generated::Answer { said: said.into(), calls: calls.iter().map(|n| call(n)).collect() }
    }

    fn script(answers: Vec<Generated>) -> Script {
        Script { answers: answers.into(), client: vec![], stop_on_call: None, log: vec![], stopped: false, corrects: true, final_answer: false }
    }

    impl Agent for Script {
        async fn generate(&mut self, step: usize) -> anyhow::Result<Generated> {
            self.log.push(format!("generate {step}"));
            self.answers.pop_front().ok_or_else(|| anyhow::anyhow!("no answer left"))
        }
        async fn call(&mut self, call: &ToolCall) -> CallRun {
            self.log.push(format!("call {}", call.name));
            if self.client.contains(&call.name.as_str()) {
                return CallRun::Client;
            }
            if self.stop_on_call == Some(call.name.as_str()) {
                self.stopped = true;
                return CallRun::Stop;
            }
            CallRun::Done(format!("{} ok", call.name))
        }
        fn record_answer(&mut self, said: &str, calls: &[ToolCall]) {
            self.log.push(format!("answer '{said}' {}", calls.len()));
        }
        fn record_result(&mut self, call: &ToolCall, result: &str) {
            self.log.push(format!("result {} {result}", call.name));
        }
        fn record_correction(&mut self, said: &str, _note: &str) -> bool {
            self.log.push(format!("correction '{said}'"));
            self.corrects
        }
        fn stopped(&self) -> bool {
            self.stopped
        }
        fn prepare_final_answer(&mut self) -> bool {
            self.log.push("final answer".into());
            self.final_answer
        }
    }

    #[tokio::test]
    async fn calls_run_in_order_and_the_model_answers_again() {
        let mut agent = script(vec![answer("let me look", &["read", "search"]), answer("done", &[])]);
        assert_eq!(run(&mut agent, 5).await.unwrap(), Ended::Answered("done".into()));
        assert_eq!(
            agent.log,
            [
                "generate 0",
                "answer 'let me look' 2",
                "call read",
                "result read read ok",
                "call search",
                "result search search ok",
                "generate 1"
            ]
        );
    }

    #[tokio::test]
    async fn a_client_function_ends_the_turn_with_the_call() {
        let mut agent = script(vec![answer("checking", &["get_weather"])]);
        agent.client = vec!["get_weather"];
        assert_eq!(
            run(&mut agent, 5).await.unwrap(),
            Ended::ClientCall { said: "checking".into(), call: call("get_weather") }
        );
    }

    #[tokio::test]
    async fn a_stop_during_a_call_ends_the_turn_and_skips_the_rest() {
        let mut agent = script(vec![answer("x", &["slow", "next"])]);
        agent.stop_on_call = Some("slow");
        assert_eq!(run(&mut agent, 5).await.unwrap(), Ended::Stopped("x".into()));
        assert!(!agent.log.iter().any(|l| l == "call next"));
    }

    #[tokio::test]
    async fn the_step_limit_ends_a_turn_that_keeps_calling() {
        let mut agent = script(vec![answer("a", &["t"]), answer("b", &["t"])]);
        assert_eq!(run(&mut agent, 2).await.unwrap(), Ended::StepLimit);
    }

    #[tokio::test]
    async fn at_the_step_limit_one_answer_without_tools_ends_the_turn() {
        let mut agent = script(vec![answer("a", &["t"]), answer("b", &["u"]), answer("from the results", &[])]);
        agent.final_answer = true;
        assert_eq!(run(&mut agent, 2).await.unwrap(), Ended::Answered("from the results".into()));
        assert_eq!(agent.log.iter().rev().take(2).cloned().collect::<Vec<_>>(), ["generate 2", "final answer"]);
    }

    #[tokio::test]
    async fn at_the_repeat_limit_too_and_a_call_in_the_final_answer_is_not_run() {
        let mut agent = script(vec![
            answer("1", &["stuck"]),
            answer("2", &["stuck"]),
            answer("3", &["stuck"]),
            answer("still", &["stuck"]),
        ]);
        agent.final_answer = true;
        assert_eq!(run(&mut agent, 0).await.unwrap(), Ended::StepLimit);
        assert_eq!(agent.log.iter().filter(|l| *l == "call stuck").count(), 1);
        assert_eq!(agent.log.last().map(String::as_str), Some("generate 0"), "{:?}", agent.log);
    }

    #[tokio::test]
    async fn a_stopped_generation_ends_the_turn() {
        let mut agent = script(vec![Generated::Stopped { said: "partial".into() }]);
        assert_eq!(run(&mut agent, 5).await.unwrap(), Ended::Stopped("partial".into()));
    }

    #[tokio::test]
    async fn distinct_calls_run_past_old_fifteen_step_limit_with_zero_unlimited() {
        let mut steps: Vec<Generated> = (1..=20)
            .map(|i| Generated::Answer {
                said: format!("step {i}"),
                calls: vec![ToolCall {
                    name: "research_read".into(),
                    arguments: format!("{{\"source\":\"S{i}\"}}"),
                    raw: format!("<call S{i}>"),
                }],
            })
            .collect();
        steps.push(answer("final report", &[]));
        let mut agent = script(steps);
        assert_eq!(run(&mut agent, 0).await.unwrap(), Ended::Answered("final report".into()));
    }

    fn call_with(name: &str, arguments: &str) -> ToolCall {
        ToolCall { name: name.into(), arguments: arguments.into(), raw: format!("<call {name} {arguments}>") }
    }

    fn answer_with(said: &str, calls: &[(&str, &str)]) -> Generated {
        Generated::Answer { said: said.into(), calls: calls.iter().map(|(n, a)| call_with(n, a)).collect() }
    }

    #[test]
    fn the_call_signature_ignores_key_order_whitespace_and_name_case() {
        let a = call_with("read_item", r#"{"kind":"memory","properties":{"unit":"celsius","city":"Paris"}}"#);
        let b = call_with(" Read_Item ", "{ \"properties\" : { \"city\": \"Paris\", \"unit\": \"celsius\" },\n  \"kind\": \"memory\" }");
        assert_eq!(call_signature(&a), call_signature(&b));
        let c = call_with("read_item", r#"{"kind":"memory","properties":{"unit":"fahrenheit","city":"Paris"}}"#);
        assert_ne!(call_signature(&a), call_signature(&c));
        // Not JSON: the trimmed text
        assert_eq!(call_signature(&call_with("x", " not json ")).1, "not json");
    }

    #[test]
    fn the_second_identical_call_reuses_and_the_third_stops() {
        let mut repeats = Repeats::default();
        let sig = call_signature(&call_with("read_item", r#"{"a":1,"b":2}"#));
        assert_eq!(repeats.check(&sig), Repeat::Run);
        repeats.ran(&sig, "first result");
        let same = call_signature(&call_with("read_item", r#"{ "b": 2, "a": 1 }"#));
        assert_eq!(repeats.check(&same), Repeat::Reuse("first result".into()));
        assert_eq!(repeats.check(&same), Repeat::Stop(Some("first result".into())));
    }

    #[test]
    fn a_new_call_starts_the_counts_again() {
        let mut repeats = Repeats::default();
        let read = call_signature(&call_with("read_file", r#"{"path":"a.rs"}"#));
        let edit = |n: u32| call_signature(&call_with("edit_file", &format!(r#"{{"path":"a.rs","text":"{n}"}}"#)));
        for n in 0..5 {
            assert_eq!(repeats.check(&read), Repeat::Run, "read {n} after an edit runs again");
            repeats.ran(&read, &format!("content {n}"));
            assert_eq!(repeats.check(&edit(n)), Repeat::Run);
            repeats.ran(&edit(n), "ok");
        }
    }

    #[tokio::test]
    async fn identical_calls_run_once_reuse_once_and_stop_on_the_third() {
        let mut agent = script(vec![
            answer("1", &["stuck_tool"]),
            answer("2", &["stuck_tool"]),
            answer("3", &["stuck_tool"]),
            answer("never", &[]),
        ]);
        assert_eq!(run(&mut agent, 0).await.unwrap(), Ended::StepLimit);
        // The tool runs once; the 2nd gets the 1st result back marked as a repeat; the 3rd stops
        assert_eq!(agent.log.iter().filter(|l| *l == "call stuck_tool").count(), 1);
        let results: Vec<&String> = agent.log.iter().filter(|l| l.starts_with("result stuck_tool")).collect();
        assert_eq!(results.len(), 3, "every call has a result: {:?}", agent.log);
        assert_eq!(results[0], "result stuck_tool stuck_tool ok");
        assert!(results[1] != results[0] && results[1].contains("stuck_tool ok"), "{}", results[1]);
        assert_eq!(agent.log.iter().filter(|l| l.starts_with("generate")).count(), 3);
    }

    #[tokio::test]
    async fn calls_that_alternate_are_caught_too() {
        // Run 77, Llama 3.2 1B: read_item with celsius, celsius, fahrenheit, celsius, ... for 100 steps
        let c = ("read_item", r#"{"kind":"memory","name":"get_weather","properties":{"city":"Paris","unit":"celsius"}}"#);
        let f = ("read_item", r#"{"kind":"memory","name":"get_weather","properties":{"city":"Paris","unit":"fahrenheit"}}"#);
        let order = [c, c, f, c, f, c, f, c, f, c];
        let mut agent = script(order.iter().enumerate().map(|(i, call)| answer_with(&i.to_string(), &[*call])).collect());
        agent.final_answer = true;
        agent.answers.truncate(7);
        agent.answers.push_back(answer("Paris weather is not in the results", &[]));
        assert_eq!(run(&mut agent, 100).await.unwrap(), Ended::Answered("Paris weather is not in the results".into()));
        // c runs, c reused, f new (counts start again), c runs, f reused, c reused, f third: stop
        assert_eq!(agent.log.iter().filter(|l| *l == "call read_item").count(), 3, "{:?}", agent.log);
        assert_eq!(agent.log.iter().filter(|l| l.starts_with("generate")).count(), 8, "7 steps and the final answer");
    }

    fn unusable(said: &str, problem: Unusable) -> Generated {
        Generated::Unusable { said: said.into(), problem }
    }

    #[tokio::test]
    async fn an_unusable_answer_is_corrected_and_the_turn_goes_on() {
        let mut agent = script(vec![
            unusable("", Unusable::NoAnswer),
            unusable("let me read", Unusable::UnreadableCall("<tool_call><function=read>".into())),
            answer("reading", &["read"]),
            unusable("", Unusable::NoAnswer),
            answer("done", &[]),
        ]);
        assert_eq!(run(&mut agent, 0).await.unwrap(), Ended::Answered("done".into()));
        assert_eq!(agent.log.iter().filter(|l| l.starts_with("correction")).count(), 3);
        assert!(agent.log.iter().any(|l| l == "call read"), "the corrected call ran: {:?}", agent.log);
    }

    #[tokio::test]
    async fn unusable_answers_end_the_turn_after_the_most_corrections() {
        let mut answers: Vec<Generated> = (0..=MOST_CORRECTIONS).map(|_| unusable("", Unusable::NoAnswer)).collect();
        answers.push(answer("never reached", &[]));
        let mut agent = script(answers);
        assert_eq!(run(&mut agent, 0).await.unwrap(), Ended::Unusable { said: String::new(), problem: Unusable::NoAnswer });
        assert_eq!(agent.log.iter().filter(|l| l.starts_with("correction")).count(), MOST_CORRECTIONS);
    }

    #[tokio::test]
    async fn a_front_end_without_corrections_ends_the_turn_at_once() {
        let mut agent = script(vec![unusable("x", Unusable::UnreadableCall("<tool_call>?".into())), answer("never", &[])]);
        agent.corrects = false;
        assert!(matches!(run(&mut agent, 0).await.unwrap(), Ended::Unusable { problem: Unusable::UnreadableCall(_), .. }));
        assert_eq!(agent.log.iter().filter(|l| l.starts_with("generate")).count(), 1);
    }

    #[test]
    fn the_note_quotes_a_long_unreadable_call_in_part() {
        let note = Unusable::UnreadableCall("x".repeat(QUOTED_CHARS * 2)).note();
        assert!(note.contains(&"x".repeat(QUOTED_CHARS)) && !note.contains(&"x".repeat(QUOTED_CHARS + 1)));
        assert!(!Unusable::NoAnswer.note().is_empty());
    }
}
