use std::{
    io::{self, Write},
    str,
};

use crate::{config::OutFormat, lint::ProjectResults};

use ariadne::{
    CharSet, Color, Config as CliConfig, Fmt, Label, LabelAttach, Report as CliReport,
    ReportKind as CliReportKind, Source,
};
use lib::Severity;
use rnix::{TextRange, TextSize};
use vfs::ReadOnlyVfs;

pub trait WriteDiagnostic {
    fn write(
        &mut self,
        report: &ProjectResults,
        vfs: &ReadOnlyVfs,
        format: OutFormat,
    ) -> io::Result<()>;
}

impl<T> WriteDiagnostic for T
where
    T: Write,
{
    fn write(
        &mut self,
        lint_result: &ProjectResults,
        vfs: &ReadOnlyVfs,
        format: OutFormat,
    ) -> io::Result<()> {
        match format {
            #[cfg(feature = "json")]
            OutFormat::Json => json::write_json(self, lint_result, vfs),
            #[cfg(feature = "json")]
            OutFormat::Marvin => marvin::write_marvin(self, lint_result, vfs),
            OutFormat::StdErr => write_stderr(self, lint_result, vfs),
            OutFormat::Errfmt => write_errfmt(self, lint_result, vfs),
        }
    }
}

fn write_stderr<T: Write>(
    writer: &mut T,
    project_results: &ProjectResults,
    vfs: &ReadOnlyVfs,
) -> io::Result<()> {
    for (file_id, reports) in project_results {
        let src = str::from_utf8(vfs.get(*file_id)).unwrap();
        let path = vfs.file_path(*file_id);
        let range = |at: TextRange| at.start().into()..at.end().into();
        let src_id = path.to_str().unwrap_or("<unknown>");
        for report in reports.iter() {
            let offset = report
                .diagnostics
                .iter()
                .map(|d| d.at.start().into())
                .min()
                .unwrap_or(0usize);
            let report_kind = match report.severity {
                Severity::Warn => CliReportKind::Warning,
                Severity::Error => CliReportKind::Error,
                Severity::Hint => CliReportKind::Advice,
            };
            report
                .diagnostics
                .iter()
                .fold(
                    CliReport::build(report_kind, src_id, offset)
                        .with_config(
                            CliConfig::default()
                                .with_cross_gap(true)
                                .with_multiline_arrows(false)
                                .with_label_attach(LabelAttach::Middle)
                                .with_char_set(CharSet::Unicode),
                        )
                        .with_message(report.note)
                        .with_code(report.code),
                    |cli_report, diagnostic| {
                        cli_report.with_label(
                            Label::new((src_id, range(diagnostic.at)))
                                .with_message(&colorize(&diagnostic.message))
                                .with_color(Color::Magenta),
                        )
                    },
                )
                .finish()
                .write((src_id, Source::from(src)), &mut *writer)?;
        }
    }
    Ok(())
}

fn write_errfmt<T: Write>(
    writer: &mut T,
    project_results: &ProjectResults,
    vfs: &ReadOnlyVfs,
) -> io::Result<()> {
    for (file_id, reports) in project_results {
        let src = str::from_utf8(vfs.get(*file_id)).unwrap();
        let path = vfs.file_path(*file_id);
        for report in reports.iter() {
            for diagnostic in report.diagnostics.iter() {
                let line = line(diagnostic.at.start(), src);
                let col = column(diagnostic.at.start(), src);
                writeln!(
                    writer,
                    "{filename}>{linenumber}:{columnnumber}:{errortype}:{errornumber}:{errormessage}",
                    filename = path.to_str().unwrap_or("<unknown>"),
                    linenumber = line,
                    columnnumber = col,
                    errortype = match report.severity {
                        Severity::Warn => "W",
                        Severity::Error => "E",
                        Severity::Hint => "I", /* "info" message */
                    },
                    errornumber = report.code,
                    errormessage = diagnostic.message
                )?;
            }
        }
    }
    Ok(())
}

#[cfg(feature = "json")]
mod json {
    use crate::lint::ProjectResults;

    use std::io::{self, Write};

    use lib::Severity;
    use rnix::TextRange;
    use serde::Serialize;
    use vfs::ReadOnlyVfs;

    #[derive(Serialize)]
    struct JsonReport<'μ> {
        note: &'static str,
        code: u32,
        severity: &'μ Severity,
        diagnostics: Vec<JsonDiagnostic<'μ>>,
    }

    #[derive(Serialize)]
    struct JsonDiagnostic<'μ> {
        at: JsonSpan,
        message: &'μ String,
        suggestion: Option<JsonSuggestion>,
    }

    #[derive(Serialize)]
    struct JsonSuggestion {
        at: JsonSpan,
        fix: String,
    }

    #[derive(Serialize)]
    struct JsonSpan {
        from: Position,
        to: Position,
    }

    #[derive(Serialize)]
    struct Position {
        line: usize,
        column: usize,
    }

    impl JsonSpan {
        fn from_textrange(at: TextRange, src: &str) -> Self {
            let start = at.start();
            let end = at.end();
            let from = Position {
                line: super::line(start, src),
                column: super::column(start, src),
            };
            let to = Position {
                line: super::line(end, src),
                column: super::column(end, src),
            };
            Self { from, to }
        }
    }

    pub fn write_json<T: Write>(
        writer: &mut T,
        project_results: &ProjectResults,
        vfs: &ReadOnlyVfs,
    ) -> io::Result<()> {
        let report = project_results
            .into_iter()
            .map(|(file_id, reports)| {
                let path = vfs.file_path(*file_id);
                let src = vfs.get_str(*file_id);
                reports.iter().map(move |r| {
                    let note = r.note;
                    let code = r.code;
                    let severity = &r.severity;
                    let diagnostics = r
                        .diagnostics
                        .iter()
                        .map(move |d| JsonDiagnostic {
                            at: JsonSpan::from_textrange(d.at, src),
                            message: &d.message,
                            suggestion: d.suggestion.as_ref().map(|s| JsonSuggestion {
                                at: JsonSpan::from_textrange(s.at, &src),
                                fix: s.fix.to_string(),
                            }),
                        })
                        .collect::<Vec<_>>();
                    JsonReport {
                        note,
                        code,
                        severity,
                        diagnostics,
                    }
                })
            })
            .flatten()
            .collect::<Vec<_>>();
        writeln!(writer, "{}", serde_json::to_string_pretty(&report).unwrap())?;
        Ok(())
    }
}

#[cfg(feature = "json")]
mod marvin {
    use crate::lint::ProjectResults;

    use std::{
        io::{self, Write},
        path::Path,
    };

    use lib::Severity;
    use rnix::TextRange;
    use serde::Serialize;
    use vfs::ReadOnlyVfs;

    #[derive(Default, Debug, Serialize)]
    pub struct AnalysisResult<'μ> {
        pub issues: Vec<Issue<'μ>>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        pub metrics: Vec<Metric>,
        pub is_passed: bool,
        pub errors: Vec<Error>,
    }

    #[derive(Debug, Serialize)]
    pub struct Issue<'μ> {
        #[serde(rename = "issue_code")]
        pub code: String,
        #[serde(rename = "issue_text")]
        pub message: String,
        pub location: Location<'μ>,
    }

    #[derive(Debug, Serialize)]
    pub struct Location<'μ> {
        pub path: &'μ Path,
        pub position: Span,
    }

    #[derive(Debug, PartialEq, Serialize)]
    pub struct Span {
        pub begin: Position,
        pub end: Position,
    }

    #[derive(Debug, PartialEq, Serialize)]
    pub struct Position {
        pub line: usize,
        pub column: usize,
    }

    #[derive(Debug, Serialize)]
    pub struct Metric {
        #[serde(rename = "metric_code")]
        pub code: String,
        pub namespaces: Vec<Namespace>,
    }

    #[derive(Debug, Serialize)]
    pub struct Namespace {
        pub key: String,
        pub value: u64,
    }

    #[derive(Debug, Serialize)]
    pub struct Error {
        pub hmessage: String,
        pub level: u64,
    }

    impl Span {
        fn from_textrange(at: TextRange, src: &str) -> Self {
            let start = at.start();
            let end = at.end();
            let begin = Position {
                line: super::line(start, src),
                column: super::column(start, src),
            };
            let end = Position {
                line: super::line(end, src),
                column: super::column(end, src),
            };
            Self { begin, end }
        }
    }

    pub fn write_marvin<T: Write>(
        writer: &mut T,
        project_results: &ProjectResults,
        vfs: &ReadOnlyVfs,
    ) -> io::Result<()> {
        let issues = project_results
            .into_iter()
            .map(|(file_id, reports)| {
                let path = vfs.file_path(*file_id);
                let src = vfs.get_str(*file_id);
                reports
                    .into_iter()
                    .map(move |r| {
                        let code = format!(
                            "NIX-{}{}",
                            match r.severity {
                                Severity::Warn | Severity::Hint => 'W',
                                Severity::Error => 'E',
                            },
                            r.code + 1000
                        );
                        r.diagnostics.iter().map(move |d| Issue {
                            code: code.clone(),
                            location: Location {
                                path,
                                position: Span::from_textrange(d.at, &src),
                            },
                            message: d.message.clone(),
                        })
                    })
                    .flatten()
            })
            .flatten()
            .collect::<Vec<_>>();
        writeln!(
            writer,
            "{}",
            serde_json::to_string_pretty(&AnalysisResult {
                issues,
                ..Default::default()
            })
            .unwrap()
        )?;
        Ok(())
    }
}

fn line(at: TextSize, src: &str) -> usize {
    let at = at.into();
    src[..at].chars().filter(|&c| c == '\n').count() + 1
}

fn column(at: TextSize, src: &str) -> usize {
    let at = at.into();
    src[..at].rfind('\n').map(|c| at - c).unwrap_or(at + 1)
}

// everything within backticks is colorized, backticks are removed
fn colorize(message: &str) -> String {
    message
        .split('`')
        .enumerate()
        .map(|(idx, part)| {
            if idx % 2 == 1 {
                part.fg(Color::Cyan).to_string()
            } else {
                part.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("")
}
