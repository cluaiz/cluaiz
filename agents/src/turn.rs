//! One agent turn, shared by every chat front end.
//!
//! The model answers; when the answer ends with tool calls, each call runs and its result joins the
//! conversation, and the model answers again. The turn ends with an answer that makes no call, a call
//! the client runs itself (OpenAI `tool_calls`), a stop (Ctrl+C, a cancelled stream, a closed
//! client), or the step limit.
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
}

/// Consecutive identical calls before a warning is returned instead of re-running the call
pub const REPEAT_WARN_THRESHOLD: usize = 3;
/// Consecutive identical calls that end the turn as stuck (`Ended::StepLimit`)
pub const REPEAT_STOP_THRESHOLD: usize = 4;
/// Unusable answers in a row the model is asked to correct; the next one ends the turn
pub const MOST_CORRECTIONS: usize = 2;

fn call_signature(call: &ToolCall) -> (String, String) {
    let canonical_args = serde_json::from_str::<serde_json::Value>(&call.arguments)
        .ok()
        .and_then(|v| serde_json::to_string(&v).ok())
        .unwrap_or_else(|| call.arguments.trim().to_owned());
    (call.name.trim().to_ascii_lowercase(), canonical_args)
}

/// Runs one turn: at most `max_steps` generations (`0` = unlimited, guarded by repeat detection)
pub async fn run<A: Agent>(agent: &mut A, max_steps: usize) -> anyhow::Result<Ended> {
    let limit = if max_steps == 0 { usize::MAX } else { max_steps };
    let mut last_sig: Option<(String, String)> = None;
    let mut repeat_count: usize = 0;
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
            if last_sig.as_ref() == Some(&sig) {
                repeat_count += 1;
            } else {
                last_sig = Some(sig);
                repeat_count = 1;
            }
            if repeat_count >= REPEAT_STOP_THRESHOLD {
                return Ok(Ended::StepLimit);
            }
            if repeat_count == REPEAT_WARN_THRESHOLD {
                let warn = format!(
                    "Repeated identical tool call '{}' {} times in a row with the same arguments. Do not repeat this call; move to the next step or provide your final answer.",
                    call.name, REPEAT_WARN_THRESHOLD
                );
                agent.record_result(call, &warn);
                continue;
            }
            match agent.call(call).await {
                CallRun::Done(result) => agent.record_result(call, &result),
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
    }

    fn call(name: &str) -> ToolCall {
        ToolCall { name: name.into(), arguments: "{}".into(), raw: format!("<call {name}>") }
    }

    fn answer(said: &str, calls: &[&str]) -> Generated {
        Generated::Answer { said: said.into(), calls: calls.iter().map(|n| call(n)).collect() }
    }

    fn script(answers: Vec<Generated>) -> Script {
        Script { answers: answers.into(), client: vec![], stop_on_call: None, log: vec![], stopped: false, corrects: true }
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

    #[tokio::test]
    async fn consecutive_identical_calls_warn_on_third_and_stop_on_fourth() {
        let mut agent = script(vec![
            answer("1", &["stuck_tool"]),
            answer("2", &["stuck_tool"]),
            answer("3", &["stuck_tool"]),
            answer("4", &["stuck_tool"]),
        ]);
        assert_eq!(run(&mut agent, 0).await.unwrap(), Ended::StepLimit);
        // The tool itself is only executed twice; the 3rd records a warning and the 4th stops
        let executed_calls = agent.log.iter().filter(|l| *l == "call stuck_tool").count();
        assert_eq!(executed_calls, 2);
        assert!(agent.log.iter().any(|l| l.contains("Repeated identical tool call 'stuck_tool' 3 times")));
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
