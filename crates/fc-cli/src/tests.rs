use super::*;
use clap::CommandFactory;
use fc_api::ErrorCode;
use fc_app::testing::TestApp;

#[test]
fn cli_definition_is_valid() {
    Cli::command().debug_assert();
}

#[test]
fn parses_commands_to_their_methods() {
    let id = "01900000-0000-7000-8000-000000000001";
    let cases: &[(&[&str], &str)] = &[
        (&["deck", "create", "Dil::İngilizce"], "decks.create"),
        (&["deck", "create", "X", "--parent", "Dil"], "decks.create"),
        (&["deck", "list"], "decks.tree"),
        (&["deck", "rename", "Dil", "Diller"], "decks.rename"),
        (&["deck", "move", "Dil::X", "--top"], "decks.move"),
        (&["deck", "delete", "Dil"], "decks.delete"),
        (&["deck", "restore", id], "decks.restore"),
        (
            &[
                "note", "add", "--deck", "A", "--front", "dog", "--back", "köpek",
            ],
            "notes.create",
        ),
        (
            &["--db", "x.db", "note", "list", "--deck", "A"],
            "notes.list",
        ),
        (&["note", "edit", id, "--front", "x"], "notes.update"),
        (&["note", "delete", id], "notes.delete"),
        (&["note", "restore", id], "notes.restore"),
        (
            &[
                "attribute",
                "define",
                "--name",
                "Seviye",
                "--kind",
                "ordered-enum",
            ],
            "attributes.define",
        ),
        (
            &[
                "attribute",
                "define",
                "--template",
                "cefr",
                "--locale",
                "en",
            ],
            "attributes.define_from_template",
        ),
        (
            &["attribute", "apply-template", "cefr", "--deck", "Dil"],
            "attributes.define_from_template",
        ),
        (
            &["attribute", "set", id, "level", "B1"],
            "attributes.values.set",
        ),
        (&["attribute", "list"], "attributes.list"),
        (
            &["attribute", "option", "add", "level", "D1"],
            "attributes.option.add",
        ),
        (
            &["attribute", "option", "rename", "level", "B1", "B1+"],
            "attributes.option.rename",
        ),
        (
            &["attribute", "option", "reorder", "level", "A2", "A1"],
            "attributes.option.reorder",
        ),
        (
            &[
                "attribute",
                "option",
                "delete",
                "level",
                "C2",
                "--replace-with",
                "C1",
            ],
            "attributes.option.delete",
        ),
        (&["plugin", "list"], "plugins.list"),
        (&["plugin", "inspect", "x.fcplugin"], "plugins.inspect"),
        (
            &["plugin", "install", "x.fcplugin", "--grant", "ui:command"],
            "plugins.install",
        ),
        (
            &["plugin", "install", "x.fcplugin", "--grant-all"],
            "plugins.install",
        ),
        (
            &["plugin", "uninstall", "a.b", "--delete-data"],
            "plugins.uninstall",
        ),
        (&["plugin", "enable", "a.b"], "plugins.set_enabled"),
        (&["plugin", "disable", "a.b"], "plugins.set_enabled"),
        (
            &["plugin", "grant", "a.b", "decks:read"],
            "plugins.set_permissions",
        ),
        (&["plugin", "logs", "a.b"], "plugins.logs"),
        (&["plugin", "run", "a.b", "list", "{}"], "plugins.run"),
        (
            &["plugin", "pack", "dir", "--out", "x.fcplugin"],
            "plugin.pack",
        ),
        (&["undo"], "oplog.undo"),
        (&["--lang", "en", "redo"], "oplog.redo"),
        (&["call", "tags.list"], "tags.list"),
    ];
    for (argv, method) in cases {
        let cli = Cli::try_parse_from(std::iter::once("fc").chain(argv.iter().copied()))
            .unwrap_or_else(|e| panic!("{argv:?}: {e}"));
        assert_eq!(cli.command.method(), *method, "{argv:?}");
    }
    let bad: &[&[&str]] = &[
        &["note", "add", "--front", "x"],
        &["attribute", "define", "--name", "x"],
        &["attribute", "define", "--template", "cefr", "--name", "x"],
        &["deck", "move", "A", "--top", "--parent", "B"],
        &["note", "delete"],
        &["--lang", "de", "undo"],
        &["plugin", "install", "x", "--grant", "a:b", "--grant-all"],
        &["plugin", "run", "a.b"],
    ];
    for argv in bad {
        assert!(
            Cli::try_parse_from(std::iter::once("fc").chain(argv.iter().copied())).is_err(),
            "{argv:?}"
        );
    }
}

fn exec_in(t: &TestApp, stdin: &str, argv: &[&str]) -> Result<String, ApiError> {
    let cli = Cli::try_parse_from(std::iter::once("fc").chain(argv.iter().copied())).unwrap();
    let ctx = commands::Ctx {
        app: &t.app,
        json: cli.json,
        text: i18n::Catalog::new(cli.lang.unwrap_or(Locale::En)),
    };
    commands::run(&ctx, &cli.command, &mut stdin.as_bytes())
}

fn exec(t: &TestApp, argv: &[&str]) -> Result<String, ApiError> {
    exec_in(t, "", argv)
}

#[test]
fn end_to_end_against_the_app() {
    let t = TestApp::mem();
    assert!(exec(&t, &["deck", "create", "Dil::İngilizce"])
        .unwrap()
        .starts_with("created deck Dil::İngilizce"));
    exec(
        &t,
        &["deck", "create", "Fiiller", "--parent", "dil::İngilizce"],
    )
    .unwrap();
    exec(
        &t,
        &[
            "attribute",
            "apply-template",
            "cefr",
            "--deck",
            "dil::İngilizce",
            "--locale",
            "tr",
        ],
    )
    .unwrap();
    let added = exec(
        &t,
        &[
            "--json",
            "note",
            "add",
            "--deck",
            "Dil::İngilizce::Fiiller",
            "--front",
            "run",
            "--back",
            "koşmak",
            "--tags",
            "fiil düzensiz",
            "--attr",
            "SEVIYE=b1",
        ],
    )
    .unwrap();
    let note: serde_json::Value = serde_json::from_str(&added).unwrap();
    let id = note["id"].as_str().unwrap().to_owned();
    assert_eq!(note["tags"], serde_json::json!(["fiil", "düzensiz"]));
    assert_eq!(note["attributes"][0]["value"]["type"], "choice");

    let tree = exec(&t, &["deck", "list"]).unwrap();
    assert!(tree.contains("    Fiiller  [new 1"), "{tree}");
    assert!(
        tree.contains("Dil  [new 1 · learn 0 · due 0 · total 1]"),
        "{tree}"
    );
    let list = exec(&t, &["note", "list", "--deck", "Dil"]).unwrap();
    assert!(list.contains("run  [fiil düzensiz]"), "{list}");

    // Key or (localised) name, case-insensitive.
    assert_eq!(
        exec(&t, &["attribute", "set", &id, "level", "C1"]).unwrap(),
        "level = C1"
    );
    assert_eq!(
        exec(&t, &["attribute", "set", &id, "Seviye"]).unwrap(),
        "level cleared"
    );
    let attrs = exec(
        &t,
        &["attribute", "list", "--deck", "Dil::İngilizce::Fiiller"],
    )
    .unwrap();
    assert!(attrs.starts_with("level  "), "{attrs}");
    let e = exec(&t, &["attribute", "set", &id, "level", "Z9"]).unwrap_err();
    assert_eq!(e.code, ErrorCode::NotFoundAttributeOption);

    // Undo prints the translated summary of what it reverted.
    assert_eq!(
        exec(&t, &["undo"]).unwrap(),
        "Undone: Set 1 classification value(s)"
    );
    assert_eq!(
        exec(&t, &["--lang", "tr", "redo"]).unwrap(),
        "Yinelendi: 1 sınıflandırma değeri ayarlandı"
    );
    exec(&t, &["undo"]).unwrap();
    exec(&t, &["undo"]).unwrap();
    assert_eq!(
        exec(&t, &["--lang", "tr", "undo"]).unwrap(),
        "Geri alındı: Not eklendi: run"
    );
    assert_eq!(exec(&t, &["note", "list"]).unwrap(), "no notes");
    assert_eq!(
        exec(&t, &["--lang", "tr", "note", "list"]).unwrap(),
        "Not yok"
    );
    assert!(exec(&t, &["--lang", "tr", "deck", "list"])
        .unwrap()
        .contains("[yeni 0 · öğreniliyor 0 · sırası gelen 0"));
    let e = exec(&t, &["note", "add", "--deck", "Yok", "--front", "x"]).unwrap_err();
    assert_eq!(e.code, ErrorCode::NotFoundDeck);
    exec(
        &t,
        &["attribute", "define", "--name", "Sayfa", "--kind", "number"],
    )
    .unwrap();
    assert!(exec(&t, &["attribute", "list"])
        .unwrap()
        .contains("sayfa  Sayfa (number)"));
}

#[test]
fn deck_note_and_option_subcommands() {
    let t = TestApp::mem();
    exec(&t, &["deck", "create", "Dil::İngilizce"]).unwrap();
    exec(&t, &["deck", "create", "Arşiv"]).unwrap();
    assert_eq!(
        exec(&t, &["deck", "rename", "Dil::İngilizce", "English"]).unwrap(),
        "renamed deck to Dil::English"
    );
    assert_eq!(
        exec(&t, &["deck", "move", "Dil::English", "--parent", "Arşiv"]).unwrap(),
        "moved deck to Arşiv::English"
    );
    assert_eq!(
        exec(&t, &["deck", "move", "Arşiv::English", "--top"]).unwrap(),
        "moved deck to English"
    );
    let deleted = exec(&t, &["deck", "delete", "Arşiv"]).unwrap();
    let id = deleted.rsplit(' ').next().unwrap().trim_end_matches('`');
    assert!(deleted.starts_with("deleted Arşiv (1 decks"), "{deleted}");
    assert!(exec(&t, &["deck", "restore", id])
        .unwrap()
        .starts_with("restored Arşiv"));

    let mut define = vec!["attribute", "define", "--name", "Zorluk", "--kind", "enum"];
    for l in ["kolay", "orta", "zor"] {
        define.extend_from_slice(&["--option", l]);
    }
    exec(&t, &define).unwrap();
    assert_eq!(
        exec(
            &t,
            &[
                "attribute",
                "option",
                "add",
                "zorluk",
                "çok zor",
                "--after",
                "ZOR"
            ]
        )
        .unwrap(),
        "zorluk  Zorluk (enum)  [kolay, orta, zor, çok zor]"
    );
    assert_eq!(
        exec(
            &t,
            &["attribute", "option", "rename", "Zorluk", "orta", "normal"]
        )
        .unwrap(),
        "zorluk  Zorluk (enum)  [kolay, normal, zor, çok zor]"
    );
    assert_eq!(
        exec(
            &t,
            &[
                "attribute",
                "option",
                "reorder",
                "zorluk",
                "çok zor",
                "zor",
                "normal",
                "kolay"
            ]
        )
        .unwrap(),
        "zorluk  Zorluk (enum)  [çok zor, zor, normal, kolay]"
    );

    let added = exec(
        &t,
        &[
            "--json",
            "note",
            "add",
            "--deck",
            "English",
            "--front",
            "go",
            "--attr",
            "zorluk=zor",
        ],
    )
    .unwrap();
    let note: serde_json::Value = serde_json::from_str(&added).unwrap();
    let nid = note["id"].as_str().unwrap().to_owned();
    assert!(exec(
        &t,
        &[
            "attribute",
            "option",
            "delete",
            "zorluk",
            "zor",
            "--replace-with",
            "çok zor"
        ]
    )
    .unwrap()
    .ends_with("(1 values changed)"));
    assert_eq!(
        exec(
            &t,
            &["note", "edit", &nid, "--back", "gitmek", "--tags", "fiil", "--attr", "zorluk="]
        )
        .unwrap(),
        format!("edited note go ({nid})")
    );
    let got: serde_json::Value = serde_json::from_str(
        &exec(&t, &["call", "notes.get", &format!(r#"{{"id":"{nid}"}}"#)]).unwrap(),
    )
    .unwrap();
    assert_eq!(got["fields"]["back"], "gitmek");
    assert_eq!(got["tags"], serde_json::json!(["fiil"]));
    assert_eq!(got["attributes"], serde_json::json!([]));
    assert_eq!(
        exec(&t, &["note", "delete", &nid]).unwrap(),
        "deleted 1 note(s)"
    );
    assert_eq!(
        exec(&t, &["note", "restore", &nid]).unwrap(),
        "restored 1 note(s)"
    );
}

#[test]
fn call_reaches_public_methods_only() {
    let t = TestApp::mem();
    let created: serde_json::Value = serde_json::from_str(
        &exec_in(
            &t,
            r#"{"path": "Dil::Fiiller"}"#,
            &["call", "decks.create", "-"],
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(created["path"], "Dil::Fiiller");
    let tree: serde_json::Value =
        serde_json::from_str(&exec(&t, &["call", "v1.decks.tree"]).unwrap()).unwrap();
    assert_eq!(tree["roots"][0]["deck"]["name"], "Dil");
    // Internal methods are refused exactly like over IPC.
    let e = exec(
        &t,
        &[
            "call",
            "decks.restore_affected",
            r#"{"decks":[],"notes":[],"cards":[]}"#,
        ],
    )
    .unwrap_err();
    assert_eq!(e.code, ErrorCode::PermissionDenied);
    let e = exec(&t, &["call", "decks.nope"]).unwrap_err();
    assert_eq!(e.code, ErrorCode::NotFoundMethod);
    let e = exec(&t, &["call", "decks.create", "{not json"]).unwrap_err();
    assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
}

#[test]
fn ambiguous_attribute_names_ask_for_a_deck() {
    let t = TestApp::mem();
    exec(&t, &["deck", "create", "A"]).unwrap();
    exec(&t, &["deck", "create", "B"]).unwrap();
    for deck in ["A", "B"] {
        exec(
            &t,
            &[
                "attribute",
                "define",
                "--name",
                "Seviye",
                "--kind",
                "enum",
                "--option",
                "x",
                "--deck",
                deck,
            ],
        )
        .unwrap();
    }
    let e = exec(&t, &["attribute", "option", "add", "seviye", "y"]).unwrap_err();
    assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
    exec(
        &t,
        &["attribute", "option", "add", "seviye", "y", "--deck", "B"],
    )
    .unwrap();
}

#[test]
fn call_runs_the_jobs_it_queues() {
    let t = TestApp::mem();
    let created: serde_json::Value = serde_json::from_str(
        &exec(&t, &["call", "decks.create", r#"{"path": "Kelimeler"}"#]).unwrap(),
    )
    .unwrap();
    let params = serde_json::json!({
        "source": {"kind": "text", "text": "Arda, porsuk\nAli, kedi"},
        "deck_id": created["id"],
    });
    let job: serde_json::Value =
        serde_json::from_str(&exec(&t, &["call", "io.import.start", &params.to_string()]).unwrap())
            .unwrap();
    assert_eq!(job["status"], "succeeded");
    assert_eq!(job["result"]["added_notes"], 2);
    let listed = exec(&t, &["note", "list", "--deck", "Kelimeler"]).unwrap();
    assert!(listed.contains("Arda"), "{listed}");
}

/// `fc plugin`: pack a folder, inspect, install with chosen grants, run, revoke, switch off,
/// uninstall and undo — the hand-written ABI v1 fixture stands in for a plugin.
#[test]
fn plugin_subcommands() {
    let t = TestApp::mem();
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("v1");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("plugin.toml"),
        "id = \"com.example.v1\"\nname = \"V1\"\nversion = \"1.0.0\"\napi_version = \"1\"\n\
         permissions = [\"ui:command\", \"decks:read\"]\n[contributes]\ncommands = [\"list\"]\n",
    )
    .unwrap();
    let wat = include_bytes!("../../fc-plugins/tests/fixtures/abi_v1.wat");
    std::fs::write(
        src.join("plugin.wasm"),
        fc_plugins::testing::wat2wasm(wat).unwrap(),
    )
    .unwrap();
    let src_s = src.display().to_string();
    let pkg = src.join("com.example.v1-1.0.0.fcplugin");
    let pkg_s = pkg.display().to_string();

    assert!(exec(&t, &["plugin", "pack", &src_s])
        .unwrap()
        .starts_with("packed com.example.v1 1.0.0"));
    assert!(pkg.is_file());
    assert_eq!(
        exec(&t, &["plugin", "list"]).unwrap(),
        "no plugins installed"
    );
    let shown = exec(&t, &["plugin", "inspect", &pkg_s]).unwrap();
    assert!(
        shown.contains("[ ] ui:command") && shown.contains("[ ] decks:read"),
        "{shown}"
    );

    let out = exec(&t, &["plugin", "install", &pkg_s, "--grant", "ui:command"]).unwrap();
    assert!(
        out.contains("[x] ui:command") && out.contains("[ ] decks:read"),
        "{out}"
    );
    let e = exec(&t, &["plugin", "run", "com.example.v1", "list"]).unwrap_err();
    assert_eq!(e.details["code"], serde_json::json!("PERMISSION_DENIED"));
    exec(
        &t,
        &[
            "plugin",
            "grant",
            "com.example.v1",
            "ui:command",
            "decks:read",
        ],
    )
    .unwrap();
    exec(&t, &["deck", "create", "Kept"]).unwrap();
    assert!(exec(&t, &["plugin", "run", "com.example.v1", "list"])
        .unwrap()
        .contains("Kept"));
    // A permission no plugin may hold cannot be granted from here either.
    assert_eq!(
        exec(&t, &["plugin", "grant", "com.example.v1", "io:write"])
            .unwrap_err()
            .code,
        ErrorCode::ValidationPluginPermission
    );
    assert!(exec(&t, &["plugin", "disable", "com.example.v1"])
        .unwrap()
        .ends_with("disabled"));
    assert_eq!(
        exec(&t, &["plugin", "run", "com.example.v1", "list"])
            .unwrap_err()
            .code,
        ErrorCode::ConflictPluginInactive
    );
    assert!(exec(&t, &["plugin", "enable", "com.example.v1"])
        .unwrap()
        .ends_with("active"));
    assert!(exec(&t, &["plugin", "logs", "com.example.v1"]).is_ok());
    assert!(exec(&t, &["plugin", "uninstall", "com.example.v1"])
        .unwrap()
        .starts_with("uninstalled V1"));
    assert_eq!(
        exec(&t, &["plugin", "list"]).unwrap(),
        "no plugins installed"
    );
    exec(&t, &["undo"]).unwrap();
    assert!(exec(&t, &["plugin", "list"])
        .unwrap()
        .starts_with("V1 1.0.0 (com.example.v1)"));
    // --grant-all takes everything the package asks for.
    let out = exec(&t, &["plugin", "install", &pkg_s, "--grant-all"]).unwrap();
    assert!(!out.contains("[ ]"), "{out}");
    // --json prints the response.
    let json: serde_json::Value =
        serde_json::from_str(&exec(&t, &["--json", "plugin", "list"]).unwrap()).unwrap();
    assert_eq!(
        json["plugins"][0]["id"],
        serde_json::json!("com.example.v1")
    );
}
