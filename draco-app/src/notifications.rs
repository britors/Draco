//! Desktop notification for long operations that end while the window is not focused.
//!
//! The interface reports only which kind of operation ended, how and how long it took; the text
//! comes from the fixed catalog below. SQL, results, connection details and file names cannot
//! reach a notification because no such value is ever passed in. This is the one piece of
//! interface text outside `frontend/dist/locales`: the operating system shows it, not the DOM.

use std::time::Duration;

use draco_core::store;
use serde::{Deserialize, Serialize};

use crate::Application;

/// Operations shorter than this never notify.
pub const LONG_OPERATION_THRESHOLD: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FinishedOperation {
    Query,
    Script,
    Explain,
    Backup,
    Restore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OperationOutcome {
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationNotification {
    pub title: String,
    pub body: String,
}

impl Application {
    /// The notification to show for a finished operation, or `None` when it was short, the
    /// window is focused or the user turned notifications off.
    pub fn operation_notification(
        &self,
        operation: FinishedOperation,
        outcome: OperationOutcome,
        duration: Duration,
        window_focused: bool,
        locale: &str,
    ) -> Option<OperationNotification> {
        if window_focused || duration < LONG_OPERATION_THRESHOLD {
            return None;
        }
        if !store::get_settings().notify_long_operations {
            return None;
        }
        Some(notification_text(operation, outcome, duration, locale))
    }
}

fn notification_text(
    operation: FinishedOperation,
    outcome: OperationOutcome,
    duration: Duration,
    locale: &str,
) -> OperationNotification {
    use FinishedOperation as Op;
    use OperationOutcome as Out;
    let time = format_duration(duration);
    let portuguese = locale.to_ascii_lowercase().starts_with("pt");
    let body = if portuguese {
        match (operation, outcome) {
            (Op::Query, Out::Succeeded) => format!("Consulta concluída em {time}"),
            (Op::Query, Out::Failed) => format!("Consulta falhou após {time}"),
            (Op::Query, Out::Cancelled) => format!("Consulta cancelada após {time}"),
            (Op::Script, Out::Succeeded) => format!("Script concluído em {time}"),
            (Op::Script, Out::Failed) => format!("Script falhou após {time}"),
            (Op::Script, Out::Cancelled) => format!("Script cancelado após {time}"),
            (Op::Explain, Out::Succeeded) => format!("Plano de execução pronto em {time}"),
            (Op::Explain, Out::Failed) => format!("Plano de execução falhou após {time}"),
            (Op::Explain, Out::Cancelled) => format!("Plano de execução cancelado após {time}"),
            (Op::Backup, Out::Succeeded) => format!("Backup concluído em {time}"),
            (Op::Backup, Out::Failed) => format!("Backup falhou após {time}"),
            (Op::Backup, Out::Cancelled) => format!("Backup cancelado após {time}"),
            (Op::Restore, Out::Succeeded) => format!("Restauração concluída em {time}"),
            (Op::Restore, Out::Failed) => format!("Restauração falhou após {time}"),
            (Op::Restore, Out::Cancelled) => format!("Restauração cancelada após {time}"),
        }
    } else {
        let subject = match operation {
            Op::Query => "Query",
            Op::Script => "Script",
            Op::Explain => "Execution plan",
            Op::Backup => "Backup",
            Op::Restore => "Restore",
        };
        match outcome {
            Out::Succeeded => format!("{subject} finished in {time}"),
            Out::Failed => format!("{subject} failed after {time}"),
            Out::Cancelled => format!("{subject} cancelled after {time}"),
        }
    };
    OperationNotification {
        title: "Draco".to_string(),
        body,
    }
}

/// `42 s`, `3 min 5 s`, `2 h 4 min`.
fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    match seconds {
        0..=119 => format!("{seconds} s"),
        120..=7199 => match seconds % 60 {
            0 => format!("{} min", seconds / 60),
            rest => format!("{} min {rest} s", seconds / 60),
        },
        _ => match (seconds / 60) % 60 {
            0 => format!("{} h", seconds / 3600),
            minutes => format!("{} h {minutes} min", seconds / 3600),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_OPERATIONS: [FinishedOperation; 5] = [
        FinishedOperation::Query,
        FinishedOperation::Script,
        FinishedOperation::Explain,
        FinishedOperation::Backup,
        FinishedOperation::Restore,
    ];
    const ALL_OUTCOMES: [OperationOutcome; 3] = [
        OperationOutcome::Succeeded,
        OperationOutcome::Failed,
        OperationOutcome::Cancelled,
    ];

    #[test]
    fn short_or_focused_operations_never_notify() {
        let app = Application::new();
        let quick = app.operation_notification(
            FinishedOperation::Query,
            OperationOutcome::Succeeded,
            Duration::from_millis(9_999),
            false,
            "en",
        );
        assert_eq!(quick, None);
        let focused = app.operation_notification(
            FinishedOperation::Backup,
            OperationOutcome::Succeeded,
            Duration::from_secs(600),
            true,
            "en",
        );
        assert_eq!(focused, None);
    }

    #[test]
    fn every_combination_has_text_in_both_languages() {
        for locale in ["en", "pt-BR"] {
            let mut bodies = std::collections::HashSet::new();
            for operation in ALL_OPERATIONS {
                for outcome in ALL_OUTCOMES {
                    let text =
                        notification_text(operation, outcome, Duration::from_secs(42), locale);
                    assert_eq!(text.title, "Draco");
                    assert!(text.body.ends_with("42 s"), "{}", text.body);
                    assert!(bodies.insert(text.body), "texts are distinct");
                }
            }
        }
        assert_eq!(
            notification_text(
                FinishedOperation::Query,
                OperationOutcome::Succeeded,
                Duration::from_secs(42),
                "pt-BR"
            )
            .body,
            "Consulta concluída em 42 s"
        );
        assert_eq!(
            notification_text(
                FinishedOperation::Restore,
                OperationOutcome::Failed,
                Duration::from_secs(42),
                "en-US"
            )
            .body,
            "Restore failed after 42 s"
        );
    }

    #[test]
    fn durations_read_naturally() {
        assert_eq!(format_duration(Duration::from_secs(10)), "10 s");
        assert_eq!(format_duration(Duration::from_secs(119)), "119 s");
        assert_eq!(format_duration(Duration::from_secs(120)), "2 min");
        assert_eq!(format_duration(Duration::from_secs(185)), "3 min 5 s");
        assert_eq!(format_duration(Duration::from_secs(7200)), "2 h");
        assert_eq!(format_duration(Duration::from_secs(7440)), "2 h 4 min");
    }
}
