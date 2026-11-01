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
