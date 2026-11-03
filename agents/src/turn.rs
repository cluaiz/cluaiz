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

    /// The user or the client stopped the turn (checked before each generation and call)
    fn stopped(&self) -> bool;
}

/// Runs one turn: at most `max_steps` generations
pub async fn run<A: Agent>(agent: &mut A, max_steps: usize) -> anyhow::Result<Ended> {
    for step in 0..max_steps {
        if agent.stopped() {
            return Ok(Ended::Stopped(String::new()));
        }
        let (said, calls) = match agent.generate(step).await? {
            Generated::Stopped { said } => return Ok(Ended::Stopped(said)),
            Generated::Answer { said, calls } if calls.is_empty() => return Ok(Ended::Answered(said)),
            Generated::Answer { said, calls } => (said, calls),
        };
        agent.record_answer(&said, &calls);
        for call in &calls {
            if agent.stopped() {
                return Ok(Ended::Stopped(said));
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
    }

    fn call(name: &str) -> ToolCall {
        ToolCall { name: name.into(), arguments: "{}".into(), raw: format!("<call {name}>") }
    }

    fn answer(said: &str, calls: &[&str]) -> Generated {
        Generated::Answer { said: said.into(), calls: calls.iter().map(|n| call(n)).collect() }
    }

    fn script(answers: Vec<Generated>) -> Script {
        Script { answers: answers.into(), client: vec![], stop_on_call: None, log: vec![], stopped: false }
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
}
