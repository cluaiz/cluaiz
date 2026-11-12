use super::*;

fn base(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cluaiz-workspace-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[test]
fn instructions_come_from_home_agents_folders_and_folder_files_nearest_last() {
    let base = base("instructions");
    let (home, repo) = (base.join("home"), base.join("repo"));
    let web = repo.join("web");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    write(&home.join("AGENTS.md"), "home rules");
    write(&repo.join(".agents/AGENTS.md"), "---\nname: constitution\ndescription: read first\n---\n\nproject constitution");
    write(&repo.join("AGENTS.md"), "root agents");
    write(&web.join(".agent/AGENTS.md"), "web legacy folder");
    write(&web.join("CLAUDE.md"), "web claude");
    // Above the repository: never read
    write(&base.join("AGENTS.md"), "unrelated");

    let workspace = Workspace::scan(&web, Homes { cluaiz: Some(&home), user: None }, &[]);
    let labels: Vec<_> = workspace.instructions.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(labels, ["~/.cluaiz/AGENTS.md", ".agents/AGENTS.md", "AGENTS.md", "web/.agent/AGENTS.md", "web/CLAUDE.md"]);
    // The front matter is metadata, not instructions
    assert_eq!(workspace.instructions[1].text, "project constitution");
    assert_eq!(workspace.root, repo);

    // The nearest files take the budget first
    let mut problems = Vec::new();
    let small = instructions::load(&web, Some(&home), 20, &mut problems);
    assert_eq!(small.last().unwrap().text, "web claude");
    assert!(small.first().unwrap().cut());
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn rules_have_triggers_and_a_nearer_rule_hides_a_farther_one() {
    let base = base("rules");
    let (user, repo) = (base.join("user"), base.join("repo"));
    let app = repo.join("app");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    // No front matter: always on, described by its first heading
    write(&repo.join(".agents/rules/proof.md"), "# Always-on rule: Proof Before Claim\n\nPaste the output.");
    write(&repo.join(".agents/rules/style.md"), "---\ntrigger: model_decision\ndescription: Code style for Rust\n---\nUse rustfmt.");
    write(&repo.join(".agents/rules/web.md"), "---\ntrigger: glob\nglobs: \"*.ts, *.tsx\"\n---\nUse strict TypeScript.");
    write(&repo.join(".agents/rules/listed.md"), "---\ntrigger: glob\nglobs: [\"docs/**\"]\ndescription: Docs\n---\nEnglish only.");
    write(&repo.join(".agents/rules/typo.md"), "---\ntrigger: sometimes\n---\nText.");
    write(&repo.join(".agents/rules/noglob.md"), "---\ntrigger: glob\ndescription: x\n---\nText.");
    write(&repo.join(".agents/rules/manual.md"), "---\ntrigger: manual\ndescription: Release steps\n---\nTag it.");
    // The nearer folder's rule of the same name wins; the older folder name is read too
    write(&app.join(".agent/rules/style.md"), "---\ntrigger: always_on\ndescription: App style\n---\nTabs.");
    write(&user.join(".agents/rules/mine.md"), "---\ndescription: My own rule\n---\nBe brief.");

    let workspace = Workspace::scan(&app, Homes { cluaiz: None, user: Some(&user) }, &[]);
    let rule = |name: &str| workspace.find(name, Some(Kind::Rule)).unwrap_or_else(|| panic!("no rule {name}"));
    assert_eq!(rule("proof").trigger, Some(Trigger::AlwaysOn));
    assert_eq!(rule("proof").description, "Always-on rule: Proof Before Claim");
    assert_eq!(rule("style").description, "App style");
    assert_eq!(rule("style").source, "app/.agent/rules");
    assert_eq!(rule("web").trigger, Some(Trigger::Glob));
    assert_eq!(rule("web").globs, ["*.ts", "*.tsx"]);
    assert_eq!(rule("listed").globs, ["docs/**"]);
    assert_eq!(rule("typo").trigger, Some(Trigger::AlwaysOn));
    assert_eq!(rule("noglob").trigger, Some(Trigger::ModelDecision));
    assert_eq!(rule("manual").trigger, Some(Trigger::Manual));
    assert_eq!(rule("mine").source, "~/.agents/rules");
    assert_eq!(rule("Manual").body().unwrap(), "Tag it.");
    assert_eq!(workspace.rules.iter().filter(|r| r.name == "style").count(), 1);
    assert!(workspace.problems.iter().any(|p| p.contains("typo.md") && p.contains("sometimes")));
    assert!(workspace.problems.iter().any(|p| p.contains("noglob.md")));
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn workflows_and_skills_are_indexed_with_name_and_description() {
    let base = base("items");
    let (user, cluaiz, repo) = (base.join("user"), base.join("cluaiz"), base.join("repo"));
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    write(&repo.join(".agents/workflows/deploy.md"), "---\ndescription: Build and deploy the site\n---\n1. npm run build\n2. npm run deploy");
    write(&repo.join(".agents/skills/react/SKILL.md"), "---\nname: react-app\ndescription: Create or change the React app\n---\nUse vite.");
    write(&cluaiz.join("tools/skills/installed/SKILL.md"), "---\nname: installed\ndescription: from the registry\n---\nx");
    write(&cluaiz.join("tools/skills/notes/SKILL.md"), "---\nname: notes\ndescription: Meeting notes\n---\nHeadings.");
    write(&user.join(".agents/skills/review/SKILL.md"), "---\nname: review\ndescription: Review a change\n---\nLook for bugs.");
    // Not a skill: no SKILL.md
    std::fs::create_dir_all(repo.join(".agents/skills/empty")).unwrap();

    let workspace = Workspace::scan(&repo, Homes { cluaiz: Some(&cluaiz), user: Some(&user) }, &["installed".to_string()]);
    let deploy = workspace.find("/deploy", Some(Kind::Workflow)).unwrap();
    assert_eq!(deploy.description, "Build and deploy the site");
    assert!(deploy.body().unwrap().starts_with("1. npm run build"));
    let skills: Vec<_> = workspace.skills.iter().map(|s| (s.name.as_str(), s.source.as_str())).collect();
    assert_eq!(skills, [("react-app", ".agents/skills"), ("notes", "~/.cluaiz/tools/skills"), ("review", "~/.agents/skills")]);
    assert_eq!(workspace.find("REACT-APP", None).unwrap().folder(), repo.join(".agents/skills/react"));
    assert_eq!(workspace.index().count(), 4);
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn outside_a_repository_only_the_working_folder_counts() {
    let base = base("norepo");
    let dir = base.join("work");
    write(&base.join(".agents/rules/far.md"), "far");
    write(&dir.join(".agents/rules/near.md"), "near");
    let workspace = Workspace::scan(&dir, Homes::default(), &[]);
    let names: Vec<_> = workspace.rules.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["near"]);
    assert_eq!(workspace.root, dir);
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn front_matter_is_split_only_when_it_is_closed() {
    let (front, body) = split_front_matter("\u{feff}---\r\nname: x\r\n---\r\nbody\r\n");
    assert_eq!(front.get("name").and_then(serde_yaml::Value::as_str), Some("x"));
    assert_eq!(body, "body");
    let (front, body) = split_front_matter("---\nnot closed\ntext");
    assert!(front.is_empty());
    assert_eq!(body, "---\nnot closed\ntext");
}

fn prompt_fixture(name: &str) -> (PathBuf, Workspace) {
    let base = base(name);
    let repo = base.join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    write(&repo.join(".agents/AGENTS.md"), &format!("Project constitution. {}", "c".repeat(3_000)));
    write(&repo.join(".agents/rules/short.md"), "# Short rule\nKeep diffs small.");
    write(&repo.join(".agents/rules/long.md"), &format!("# Long doctrine\n{}", "d".repeat(6_000)));
    write(&repo.join(".agents/rules/style.md"), "---\ntrigger: model_decision\ndescription: Rust code style\n---\nUse rustfmt.");
    write(&repo.join(".agents/rules/web.md"), "---\ntrigger: glob\nglobs: \"*.ts\"\ndescription: TypeScript rules\n---\nStrict mode.");
    write(&repo.join(".agents/rules/release.md"), "---\ntrigger: manual\ndescription: Release steps\n---\nTag it.");
    write(&repo.join(".agents/workflows/deploy.md"), "---\ndescription: Build and deploy\n---\n1. build");
    write(&repo.join(".agents/skills/react/SKILL.md"), "---\nname: react-app\ndescription: React app work\n---\nUse vite.");
    let workspace = Workspace::scan(&repo, Homes::default(), &[]);
    (base, workspace)
}

#[test]
fn a_large_context_gets_everything_whole_and_the_index() {
    let (base, workspace) = prompt_fixture("prompt-large");
    let prompt = workspace.prompt(budget_for(32_768));
    let text = &prompt.text;
    assert!(text.contains("Project constitution"));
    assert!(text.contains("## short\n# Short rule\nKeep diffs small."));
    assert!(text.contains(&"d".repeat(6_000)), "a long always-on rule fits a large context whole");
    assert!(text.contains("- style (rule): Rust code style"));
    assert!(text.contains("- web (rule) (for files *.ts): TypeScript rules"));
    assert!(text.contains("- deploy (workflow): Build and deploy"));
    assert!(text.contains("- react-app (skill): React app work"));
    assert!(!text.contains("Release steps"), "a manual rule is only used when the user names it");
    assert!(text.len() <= prompt.budget);
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn a_small_context_keeps_the_index_cuts_instructions_and_never_cuts_a_rule() {
    let (base, workspace) = prompt_fixture("prompt-small");
    let prompt = workspace.prompt(2_000);
    let text = &prompt.text;
    let part = |name: &str| prompt.parts.iter().find(|p| p.name == name).unwrap().placement;
    // The index is placed first
    assert_eq!(part("react-app"), Placement::Index);
    assert_eq!(part("deploy"), Placement::Index);
    // The instructions are cut and say how to read the rest
    assert_eq!(part(".agents/AGENTS.md"), Placement::Cut);
    assert!(text.contains("call read_item with \".agents/AGENTS.md\""));
    // Rules that do not fit whole are listed in the index, not cut
    assert_eq!(part("long"), Placement::Index);
    assert!(text.contains("- long (rule, always on): Long doctrine"));
    assert!(!text.contains("dddd"));
    assert_eq!(part("release"), Placement::Left);
    assert!(text.len() <= prompt.budget, "{} > {}", text.len(), prompt.budget);

    // read_item reads any of them whole, and an instruction file by its label
    let (summary, rule) = workspace.read_item("long", None).unwrap();
    assert!(summary.starts_with("long"));
    assert!(rule.contains(&"d".repeat(6_000)));
    assert!(workspace.read_item(".agents/AGENTS.md", None).unwrap().1.contains(&"c".repeat(3_000)));
    assert!(workspace.read_item("/deploy", None).unwrap().1.starts_with("Workflow deploy:"));
    assert!(workspace.read_item("nothing", None).unwrap_err().contains("react-app"));
    assert!(workspace.read_item_schema().is_some());
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn an_empty_folder_adds_nothing() {
    let base = base("prompt-empty");
    let workspace = Workspace::scan(&base, Homes::default(), &[]);
    assert!(workspace.prompt(8_192).text.is_empty());
    assert!(workspace.read_item_schema().is_none());
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn the_prompt_stays_within_the_budget_and_a_shared_name_is_read_by_kind() {
    let (base, workspace) = prompt_fixture("prompt-budget");
    write(&workspace.root.join(".agents/skills/short/SKILL.md"), "---\nname: short\ndescription: A skill named like a rule\n---\nSkill text.");
    let workspace = Workspace::scan(&workspace.root, Homes::default(), &[]);
    for budget in [300, 800, 1_500, 4_096, 9_000, 20_000] {
        let prompt = workspace.prompt(budget);
        assert!(prompt.text.len() <= budget, "budget {budget}: {} characters", prompt.text.len());
    }
    assert!(workspace.read_item("short", Some("skill")).unwrap().1.contains("Skill text."));
    assert!(workspace.read_item("short", Some("rule")).unwrap().1.contains("Keep diffs small."));
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn a_confirmed_fact_is_a_file_in_the_project_and_a_newer_one_replaces_it() {
    let base = base("memory");
    let (user, repo) = (base.join("user"), base.join("repo"));
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    // The user's own folder has no memory of the project
    write(&user.join(".agents/memory/mine.md"), "Not this project.");
    let homes = Homes { cluaiz: None, user: Some(&user) };
    let workspace = Workspace::scan(&repo, homes, &[]);
    assert!(workspace.memory.is_empty());

    assert_eq!(memory::slug("Deploy Port!"), "deploy-port");
    assert_eq!(memory::slug("पोर्ट"), "fact");
    assert!(memory::prepare("x", "  ").is_err());
    assert!(memory::prepare("x", &"a".repeat(memory::MAX_FACT_CHARS + 1)).is_err());

    let first = memory::save(&workspace, "deploy-port", "The deploy server port is 8011.", "test", None).unwrap();
    assert_eq!(first.path, repo.join(".agents/memory/deploy-port.md"));
    assert!(first.replaced.is_empty());
    let workspace = Workspace::scan(&repo, homes, &[]);
    let fact = workspace.find("deploy-port", Some(Kind::Memory)).unwrap();
    assert_eq!(fact.description, "The deploy server port is 8011.");
    let text = std::fs::read_to_string(&first.path).unwrap();
    assert!(text.contains("status: confirmed") && text.contains("source: test") && text.contains("created: "));
    // In the index, as the fact itself, and read by name
    assert!(workspace.prompt(4_096).text.contains("- deploy-port (memory): The deploy server port is 8011."));
    assert!(workspace.read_item("deploy-port", Some("memory")).unwrap().1.contains("port is 8011"));

    // A newer fact that may repeat it: shown first, by name or by words; nothing is replaced by guessing
    let shown = |name: &str, fact: &str| memory::similar(&workspace, name, fact, 3).iter().map(|item| item.name.clone()).collect::<Vec<_>>();
    assert_eq!(shown("server-port", "The deploy server port is now 9090."), ["deploy-port"]);
    assert_eq!(shown("deploy-port", "Something else entirely"), ["deploy-port"]);
    assert!(shown("biryani", "A recipe for biryani").is_empty());
    assert!(memory::similar_text(&memory::similar(&workspace, "x", "deploy server port 9090", 3)).contains("- deploy-port: The deploy server port is 8011."));

    // Named: the same file gets the new text; no second file, nothing marked superseded
    let second = memory::save(&workspace, "server-port", "The deploy server port is now 9090.", "test", Some("deploy-port")).unwrap();
    assert_eq!(second.path, first.path);
    assert_eq!(second.replaced, ["deploy-port"]);
    let text = std::fs::read_to_string(&first.path).unwrap();
    assert!(text.contains("port is now 9090") && !text.contains("8011") && text.contains("updated: "));
    let workspace = Workspace::scan(&repo, homes, &[]);
    assert_eq!(workspace.memory.len(), 1);
    assert!(memory::save(&workspace, "x", "y", "test", Some("no-such-fact")).is_err());

    // A separate fact with a taken name gets its own file
    let third = memory::save(&workspace, "deploy-port", "Tests use the build profile release.", "test", None).unwrap();
    assert_eq!(third.path, repo.join(".agents/memory/deploy-port-2.md"));
    // Forgotten: the file is gone
    let workspace = Workspace::scan(&repo, homes, &[]);
    assert_eq!(memory::forget(&workspace, "deploy-port-2").unwrap(), third.path);
    assert!(!third.path.exists());
    assert!(memory::forget(&workspace, "deploy-port-2").is_err() || !third.path.exists());
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn glob_rules_apply_to_matching_files() {
    use matching::glob_matches;
    assert!(glob_matches("*.ts", "web/src/app.ts"));
    assert!(!glob_matches("*.ts", "web/src/app.tsx"));
    assert!(glob_matches("*.{ts,tsx}", "app.tsx"));
    assert!(glob_matches("docs/**", "docs/api/a.md"));
    assert!(!glob_matches("docs/**", "src/docs.md"));
    assert!(glob_matches("src/*.rs", "src/main.rs"));
    assert!(!glob_matches("src/*.rs", "src/agent/main.rs"));
    assert!(glob_matches("**/*.rs", "src/agent/main.rs"));
    assert!(glob_matches("./src/*.rs", r"src\main.rs"));

    let base = base("globs");
    std::fs::create_dir_all(base.join(".git")).unwrap();
    write(&base.join(".agents/rules/web.md"), "---\ntrigger: glob\nglobs: \"*.ts, *.tsx\"\n---\nUse strict TypeScript.");
    write(&base.join(".agents/rules/docs.md"), "---\ntrigger: glob\nglobs: [\"docs/**\"]\ndescription: Docs\n---\nEnglish only.");
    write(&base.join(".agents/rules/always.md"), "Always.");
    let workspace = Workspace::scan(&base, Homes::default(), &[]);
    let names = |path: &str| workspace.rules_for_file(path).iter().map(|rule| rule.name.clone()).collect::<Vec<_>>();
    assert_eq!(names("web/app.tsx"), ["web"]);
    assert_eq!(names("docs/guide.md"), ["docs"]);
    assert!(names("README.md").is_empty());
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn items_that_fit_a_message_go_into_its_context_block() {
    let base = base("matching");
    std::fs::create_dir_all(base.join(".git")).unwrap();
    let long = "\nShow the numbers.".repeat(25);
    write(&base.join(".agents/rules/cerd.md"), &format!("---\ntrigger: model_decision\ndescription: Check every result with real data\n---\nRun it and paste the output.{long}"));
    write(&base.join(".agents/rules/release.md"), "---\ntrigger: manual\ndescription: Release steps\n---\nTag it.");
    write(&base.join(".agents/skills/pdf-report/SKILL.md"), "---\ndescription: Make a PDF report from a table\n---\nUse the report template.");
    write(&base.join(".agents/workflows/deploy.md"), "---\ndescription: Deploy the site\n---\n1. Build.\n2. Upload.");
    let workspace = Workspace::scan(&base, Homes::default(), &[]);
    memory::save(&workspace, "deploy-port", "The deploy server port is 9090.", "test", None).unwrap();
    let workspace = Workspace::scan(&base, Homes::default(), &[]);
    let known = std::collections::HashSet::new();
    let names = |message: &str, known: &std::collections::HashSet<String>| {
        workspace.matched(message, None, known).iter().map(|item| item.name.clone()).collect::<Vec<_>>()
    };
    assert_eq!(names("Use the cerd rule for this change", &known), ["cerd"]);
    assert_eq!(names("make a pdf report of the sales", &known), ["pdf-report"]);
    // The fact first; the workflow named "deploy" comes along, as its name is said
    assert_eq!(names("which deploy port does the server use?", &known), ["deploy-port", "deploy"]);
    // Nothing that only shares a common word
    assert!(names("hello, how are you?", &known).is_empty());
    assert!(names("check this", &known).is_empty());
    // A manual rule only when named
    assert_eq!(names("follow the release rule now", &known), ["release"]);
    // Already sent (or whole in the system prompt): not again
    let sent: std::collections::HashSet<String> = ["rule:cerd".to_string()].into();
    assert!(names("Use the cerd rule for this change", &sent).is_empty());

    let items = workspace.matched("Use the cerd rule for this change", None, &known);
    let (block, keys) = workspace.context_block(&items, 2_000).unwrap();
    assert!(block.starts_with("<context>\n") && block.ends_with("</context>"));
    assert!(block.contains("## cerd (rule)\nRun it and paste the output."), "{block}");
    assert_eq!(keys, ["rule:cerd"]);
    // No room for the text: its name and how to read it, never a cut text
    let (small, keys) = workspace.context_block(&items, 300).unwrap();
    assert!(small.contains("read_item") && !small.contains("paste the output"), "{small}");
    assert!(keys.is_empty());
    assert!(workspace.context_block(&items, 10).is_none());
    // A skill says where its own files are, as read_item does: its text links them
    let items = workspace.matched("make a pdf report of the sales", None, &known);
    let (block, _) = workspace.context_block(&items, 2_000).unwrap();
    let folder = items[0].folder();
    assert!(folder.ends_with("pdf-report") && folder.is_dir());
    assert!(block.contains(&format!("## pdf-report (skill; its files are in {})", folder.display())), "{block}");
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn a_plugin_or_mcp_package_with_a_skill_file_is_in_the_index() {
    let root = std::env::temp_dir().join(format!("cluaiz-index-{}", uuid::Uuid::new_v4()));
    let project = root.join("project");
    std::fs::create_dir_all(&project).unwrap();
    let cluaiz = root.join("cluaiz");
    write(&cluaiz.join("tools/mcp/browser/SKILL.md"), "---\nname: browser\ndescription: Uses a browser\n---\nSteps.");
    write(&cluaiz.join("tools/plugins/math/SKILL.md"), "---\nname: math\ndescription: Does math\n---\nSteps.");
    std::fs::create_dir_all(cluaiz.join("tools/mcp/git")).unwrap();
    let workspace = Workspace::scan(&project, Homes { cluaiz: Some(&cluaiz), user: None }, &[]);
    let names: Vec<&str> = workspace.skills.iter().map(|skill| skill.name.as_str()).collect();
    assert_eq!(names, ["math", "browser"]);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_tool_skill_matches_on_its_package_triggers() {
    let root = std::env::temp_dir().join(format!("cluaiz-triggers-{}", uuid::Uuid::new_v4()));
    let project = root.join("project");
    std::fs::create_dir_all(&project).unwrap();
    let cluaiz = root.join("cluaiz");
    write(&cluaiz.join("tools/plugins/text/SKILL.md"), "---
name: text
description: Exact text work
---
Steps.");
    write(&cluaiz.join("tools/plugins/text/package.json"), r#"{"triggers": ["sha256 hash", "hash", "base64"]}"#);
    let workspace = Workspace::scan(&project, Homes { cluaiz: Some(&cluaiz), user: None }, &[]);
    assert_eq!(workspace.skills[0].triggers, ["sha256 hash", "hash", "base64"]);
    assert_eq!(workspace.matched_skills("give me the sha256 hash of abc", None), ["text"]);
    assert_eq!(workspace.matched_skills("What is the SHA-256 hash of the text abc? Use the text tool.", None), ["text"]);
    let _ = std::fs::remove_dir_all(&root);
}
