pub mod ast;
pub mod lexer;
pub mod planner;
pub mod eval;
pub mod metadata_parser;

pub fn parse_cel(input: &str) -> Result<planner::ExecutionPlan, String> {
    let ast = lexer::parse(input)?;
    planner::CelPlanner::new().build_plan(&ast)
}
