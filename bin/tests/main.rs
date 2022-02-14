use lib::session::{SessionInfo, Version};

macro_rules! session_info {
    ($version:expr) => {{
        let v: Version = $version.parse().unwrap();
        SessionInfo::from_version(v)
    }};
}

mod util {
    #[macro_export]
    macro_rules! test_lint {
        ($tname:ident => $sess:expr, $($tail:tt)*) => {
            test_lint!($tname => $sess);
            test_lint!($($tail)*);
        };
        ($tname:ident, $($tail:tt)*) => {
                test_lint!($tname);
                test_lint!($($tail)*);
        };
        ($tname:ident) => {
            test_lint!($tname => session_info!("2.6"));
        };
        ($tname:ident => $sess:expr) => {
            #[test]
            fn $tname() {
                use statix::{config::OutFormat, traits::WriteDiagnostic, lint::{self, ProjectResults, LintResult}};
                use vfs::ReadOnlyVfs;

                let file_path = concat!("data/", stringify!($tname), ".nix");
                let contents = include_str!(concat!("data/", stringify!($tname), ".nix"));

                let vfs = ReadOnlyVfs::singleton(file_path, contents.as_bytes());

                let session = $sess;

                let mut buffer = Vec::new();
                let results = vfs.iter().map(|entry| lint::lint(entry, &session));
                let project_results = results.fold(
                    std::collections::HashMap::new(),
                    |mut map: ProjectResults, mut item: LintResult| {
                        map.entry(item.file_id)
                            .and_modify(|reports| reports.append(&mut item.reports))
                            .or_insert(item.reports);
                        map
                    },
                );
                buffer.write(&project_results, &vfs, OutFormat::StdErr).unwrap();


                let stripped = strip_ansi_escapes::strip(&buffer).unwrap();
                let out =  std::str::from_utf8(&stripped).unwrap();
                insta::assert_snapshot!(&out);
            }
        };
    }
}

test_lint! {
    bool_comparison,
    empty_let_in,
    manual_inherit,
    manual_inherit_from,
    legacy_let_syntax,
    collapsible_let_in,
    eta_reduction,
    useless_parens,
    empty_pattern,
    redundant_pattern_bind,
    unquoted_uri,
    deprecated_is_null,
    empty_inherit,
    faster_groupby => session_info!("2.5"),
    faster_zipattrswith => session_info!("2.6"),
    deprecated_to_path => session_info!("2.4")
}
