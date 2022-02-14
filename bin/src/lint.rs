use std::collections::HashMap;

use crate::{utils, LintMap};

use lib::{session::SessionInfo, Report};
use rnix::WalkEvent;
use vfs::{FileId, VfsEntry};

#[derive(Debug)]
pub struct LintResult {
    pub file_id: FileId,
    pub reports: Vec<Report>,
}

pub type ProjectResults = HashMap<FileId, Vec<Report>>;

pub fn lint_with(vfs_entry: VfsEntry, lints: &LintMap, sess: &SessionInfo) -> LintResult {
    let file_id = vfs_entry.file_id;
    let source = vfs_entry.contents;
    let parsed = rnix::parse(source);

    let error_reports = parsed.errors().into_iter().map(Report::from_parse_err);
    let reports = parsed
        .node()
        .preorder_with_tokens()
        .filter_map(|event| match event {
            WalkEvent::Enter(child) => lints.get(&child.kind()).map(|rules| {
                rules
                    .iter()
                    .filter_map(|rule| rule.validate(&child, sess))
                    .collect::<Vec<_>>()
            }),
            _ => None,
        })
        .flatten()
        .chain(error_reports)
        .collect();

    LintResult { file_id, reports }
}

pub fn lint(vfs_entry: VfsEntry, sess: &SessionInfo) -> LintResult {
    lint_with(vfs_entry, &utils::lint_map(), &sess)
}

pub mod main {
    use std::{collections::HashMap, io};

    use super::{lint_with, LintResult, ProjectResults};
    use crate::{
        config::{Check as CheckConfig, ConfFile},
        err::StatixErr,
        traits::WriteDiagnostic,
    };

    use lib::session::SessionInfo;

    pub fn main(check_config: CheckConfig) -> Result<(), StatixErr> {
        let vfs = check_config.vfs()?;
        let mut stdout = io::stdout();
        let conf_file = ConfFile::discover(&check_config.conf_path)?;
        let lints = conf_file.lints();
        let version = conf_file.version()?;
        let session = SessionInfo::from_version(version);
        let lint = |vfs_entry| lint_with(vfs_entry, &lints, &session);
        let results = vfs.iter().map(lint);
        let project_results = results.fold(
            HashMap::new(),
            |mut map: ProjectResults, mut item: LintResult| {
                map.entry(item.file_id)
                    .and_modify(|reports| reports.append(&mut item.reports))
                    .or_insert(item.reports);
                map
            },
        );

        if !project_results.is_empty() {
            stdout
                .write(&project_results, &vfs, check_config.format)
                .unwrap();
            std::process::exit(1);
        }

        std::process::exit(0);
    }
}
