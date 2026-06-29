// Copyright (c) cluaiz Technologies.
//
//! cluaiz Expression Language (CEL) — End-to-End Quality & Benchmark Tests
//!
//! Tests the complete CEL pipeline:
//!   CEL string → parse() → build_plan() → execution/matching
//!
//! Measures parsing latency, execution times, and prints detailed telemetry
//! to align with cluaiz ENGINEERING REALITY DOCTRINE (CERD) Law 10.
//!
//! Run with:
//!   cargo test --package inference-cel --test cel_quality_mechanics -- --nocapture

#[cfg(test)]
mod cel_quality_mechanics {
    use std::time::Instant;
    use inference_cel::{
        parse_cel,
        CelValue,
        parser::ast::{CelAst, CelOp, CompareOp},
        parser::lexer,
        parser::planner::CelPlanner,
    };

    // ─────────────────────────────────────────────────────────────────────────
    // HELPERS & BENCHMARK LOGGER
    // ─────────────────────────────────────────────────────────────────────────

    fn benchmark_cel_pipeline<F, R>(label: &str, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let start = Instant::now();
        let result = f();
        let elapsed = start.elapsed();
        println!(
            "[CEL BENCHMARK] {:<40} | Time: {:<12?} | Status: PASSED",
            label, elapsed
        );
        result
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 1. PIPELINE PARSING & PLANNING TESTS (EXHAUSTIVE FEATURES CHECK)
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_pipeline_basic_parsing_quality() {
        let cel_input = "use plugin::database -> find User(id: 42) -> select(username, email)";
        
        let plan = benchmark_cel_pipeline("Parse & Plan: Basic Query", || {
            parse_cel(cel_input).expect("Failed to parse basic CEL")
        });

        assert_eq!(plan.blocks.len(), 1);
        if let inference_cel::parser::planner::PlanBlock::Pipeline(pipe_plan) = &plan.blocks[0] {
            assert_eq!(pipe_plan.steps.len(), 3);
            
            // Step 1: LoadPlugin
            match &pipe_plan.steps[0] {
                inference_cel::parser::planner::PlanStep::LoadPlugin { name } => {
                    assert_eq!(name, "database");
                }
                other => panic!("Expected LoadPlugin, found {:?}", other),
            }

            // Step 2: ExecuteCommand
            match &pipe_plan.steps[1] {
                inference_cel::parser::planner::PlanStep::ExecuteCommand { action, target, args } => {
                    assert_eq!(action, "find");
                    assert_eq!(target.as_deref(), Some("User"));
                    assert_eq!(args.get("id"), Some(&CelValue::Number(42.0)));
                }
                other => panic!("Expected ExecuteCommand, found {:?}", other),
            }

            // Step 3: Select fields
            match &pipe_plan.steps[2] {
                inference_cel::parser::planner::PlanStep::Select { fields } => {
                    assert_eq!(fields, &vec!["username".to_string(), "email".to_string()]);
                }
                other => panic!("Expected Select, found {:?}", other),
            }
        } else {
            panic!("Expected PlanBlock::Pipeline");
        }
    }

    #[test]
    fn test_pipeline_complex_control_flow_quality() {
        let cel_input = r#"
            let $user = use plugin::auth -> verify(token: "session_123");
            if ($user.is_valid) {
                use plugin::database -> find User(id: 101);
            } else {
                process("Access Denied");
            }
        "#;

        let plan = benchmark_cel_pipeline("Parse & Plan: Complex Control Flow", || {
            parse_cel(cel_input).expect("Failed to parse complex control flow")
        });

        assert_eq!(plan.blocks.len(), 2);

        // Block 1: Assignment
        match &plan.blocks[0] {
            inference_cel::parser::planner::PlanBlock::Assignment { var_name, pipeline } => {
                assert_eq!(var_name, "$user");
                assert_eq!(pipeline.steps.len(), 2);
            }
            other => panic!("Expected Assignment block, found {:?}", other),
        }

        // Block 2: IfElse
        match &plan.blocks[1] {
            inference_cel::parser::planner::PlanBlock::IfElse { condition, if_plan, else_plan } => {
                assert_eq!(condition, "$user.is_valid");
                assert_eq!(if_plan.blocks.len(), 1);
                assert!(else_plan.is_some());
            }
            other => panic!("Expected IfElse block, found {:?}", other),
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 2. EXHAUSTIVE CELOP FEATURES COVERAGE
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_celop_invoke_action() {
        let cel_input = "use plugin::web -> invoke(method: \"fetch\", payload: \"https://example.com\")";
        let plan = benchmark_cel_pipeline("Parse & Plan: InvokeAction (Plugin call)", || {
            parse_cel(cel_input).expect("Failed to parse invoke call")
        });
        if let inference_cel::parser::planner::PlanBlock::Pipeline(pipe_plan) = &plan.blocks[0] {
            assert_eq!(pipe_plan.steps.len(), 2);
            match &pipe_plan.steps[1] {
                inference_cel::parser::planner::PlanStep::ExecuteAction { method, args } => {
                    assert_eq!(method, "fetch");
                    assert_eq!(args.get("payload"), Some(&CelValue::Text("https://example.com".to_string())));
                }
                other => panic!("Expected ExecuteAction, found {:?}", other),
            }
        }
    }

    #[test]
    fn test_celop_time_window_and_similar_to() {
        let cel_input = "use plugin::cache -> time_window(size: \"2h\") -> similar_to(vector: [0.1, 0.2, 0.3], metric: \"cosine\")";
        let plan = benchmark_cel_pipeline("Parse & Plan: TimeWindow & Vector Search", || {
            parse_cel(cel_input).expect("Failed to parse time window / vector search")
        });
        if let inference_cel::parser::planner::PlanBlock::Pipeline(pipe_plan) = &plan.blocks[0] {
            assert_eq!(pipe_plan.steps.len(), 3);
            
            match &pipe_plan.steps[1] {
                inference_cel::parser::planner::PlanStep::TimeWindow { size } => {
                    assert_eq!(size, "2h");
                }
                other => panic!("Expected TimeWindow, found {:?}", other),
            }

            match &pipe_plan.steps[2] {
                inference_cel::parser::planner::PlanStep::VectorScan { vector, metric } => {
                    assert_eq!(vector, &vec![0.1, 0.2, 0.3]);
                    assert_eq!(metric, "cosine");
                }
                other => panic!("Expected VectorScan, found {:?}", other),
            }
        }
    }

    #[test]
    fn test_celop_pipe_operator() {
        let cel_input = "use plugin::crawler -> next_worker";
        let plan = benchmark_cel_pipeline("Parse & Plan: Pipe (Stream chaining)", || {
            parse_cel(cel_input).expect("Failed to parse pipe command")
        });
        if let inference_cel::parser::planner::PlanBlock::Pipeline(pipe_plan) = &plan.blocks[0] {
            assert_eq!(pipe_plan.steps.len(), 2);
            match &pipe_plan.steps[1] {
                inference_cel::parser::planner::PlanStep::Pipe { next_plugin } => {
                    assert_eq!(next_plugin, "next_worker");
                }
                other => panic!("Expected Pipe, found {:?}", other),
            }
        }
    }

    #[test]
    fn test_celop_engine_memory_control() {
        let cel_input = "engine -> kv_cache -> clear(session_999)";
        let plan = benchmark_cel_pipeline("Parse & Plan: Engine Memory Control", || {
            parse_cel(cel_input).expect("Failed to parse Engine Memory Control")
        });
        if let inference_cel::parser::planner::PlanBlock::Pipeline(pipe_plan) = &plan.blocks[0] {
            assert_eq!(pipe_plan.steps.len(), 1);
            match &pipe_plan.steps[0] {
                inference_cel::parser::planner::PlanStep::EngineMemoryControl { action, target } => {
                    assert_eq!(action, "clear");
                    assert_eq!(target, "session_999");
                }
                other => panic!("Expected EngineMemoryControl, found {:?}", other),
            }
        }
    }

    #[test]
    fn test_celop_mid_layer_injection() {
        let cel_input = "engine -> mid_layer -> inject(\"prompt_bias\")";
        let plan = benchmark_cel_pipeline("Parse & Plan: MidLayer Control", || {
            parse_cel(cel_input).expect("Failed to parse mid layer injection")
        });
        if let inference_cel::parser::planner::PlanBlock::Pipeline(pipe_plan) = &plan.blocks[0] {
            assert_eq!(pipe_plan.steps.len(), 1);
            match &pipe_plan.steps[0] {
                inference_cel::parser::planner::PlanStep::MidLayerInjection { payload } => {
                    assert_eq!(payload, &CelValue::Text("prompt_bias".to_string()));
                }
                other => panic!("Expected MidLayerInjection, found {:?}", other),
            }
        }
    }

    #[test]
    fn test_celop_inference_control() {
        let cel_input = "engine -> inference -> pause()";
        let plan = benchmark_cel_pipeline("Parse & Plan: Inference Control", || {
            parse_cel(cel_input).expect("Failed to parse inference control")
        });
        if let inference_cel::parser::planner::PlanBlock::Pipeline(pipe_plan) = &plan.blocks[0] {
            assert_eq!(pipe_plan.steps.len(), 1);
            match &pipe_plan.steps[0] {
                inference_cel::parser::planner::PlanStep::InferenceControl { command } => {
                    assert_eq!(command, "pause");
                }
                other => panic!("Expected InferenceControl, found {:?}", other),
            }
        }
    }

    #[test]
    fn test_celop_system_call() {
        let cel_input = "engine -> os -> process(\"nvidia-smi\")";
        let plan = benchmark_cel_pipeline("Parse & Plan: SystemCall Directive", || {
            parse_cel(cel_input).expect("Failed to parse SystemCall")
        });
        if let inference_cel::parser::planner::PlanBlock::Pipeline(pipe_plan) = &plan.blocks[0] {
            assert_eq!(pipe_plan.steps.len(), 1);
            match &pipe_plan.steps[0] {
                inference_cel::parser::planner::PlanStep::SystemCall { command, args } => {
                    assert_eq!(command, "process");
                    assert_eq!(args.len(), 1);
                    assert_eq!(args[0], CelValue::Text("nvidia-smi".to_string()));
                }
                other => panic!("Expected SystemCall, found {:?}", other),
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 3. SECURITY GATES
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_parser_nesting_depth_overflow_mitigation() {
        let mut deep_cel = String::new();
        for _ in 0..40 {
            deep_cel.push_str("if (true) { ");
        }
        deep_cel.push_str("process('nested');");
        for _ in 0..40 {
            deep_cel.push_str(" } ");
        }

        benchmark_cel_pipeline("Security Check: Stack Overflow Block", || {
            let parse_result = lexer::parse(&deep_cel);
            assert!(parse_result.is_err(), "Deep nesting must fail validation");
            let err_msg = parse_result.err().unwrap();
            assert!(
                err_msg.contains("nesting depth exceeded"),
                "Expected nesting depth error, got: {}", err_msg
            );
        });
    }

    #[test]
    fn test_plugin_name_sanitization_guards() {
        let malicious_cel = "use plugin::../../etc/passwd -> process('attack')";

        benchmark_cel_pipeline("Security Check: Path Traversal Block", || {
            let parse_result = lexer::parse(malicious_cel);
            assert!(parse_result.is_err(), "Malicious plugin path must fail parsing");
            let err_msg = parse_result.err().unwrap();
            assert!(
                err_msg.contains("invalid characters"),
                "Expected invalid characters error, got: {}", err_msg
            );
        });
    }

    #[test]
    fn test_pipeline_filter_comparisons() {
        let cel_input = "use plugin::db -> filter age >= 18, status == 'active'";

        let plan = benchmark_cel_pipeline("Parse & Plan: Comparison Filters", || {
            parse_cel(cel_input).expect("Failed to parse filter expression")
        });

        if let inference_cel::parser::planner::PlanBlock::Pipeline(pipe_plan) = &plan.blocks[0] {
            assert_eq!(pipe_plan.steps.len(), 2);
        }
    }
}
