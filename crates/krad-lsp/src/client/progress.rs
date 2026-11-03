use lsp_types::{NumberOrString, ProgressParams, ProgressParamsValue, WorkDoneProgress};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    pub token: String,
    pub state: ProgressState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressState {
    Begin {
        title: String,
        message: Option<String>,
        percentage: Option<u32>,
    },
    Report {
        message: Option<String>,
        percentage: Option<u32>,
    },
    End {
        message: Option<String>,
    },
}

impl From<ProgressParams> for Progress {
    fn from(params: ProgressParams) -> Self {
        let token = match params.token {
            NumberOrString::Number(number) => number.to_string(),
            NumberOrString::String(text) => text,
        };
        let ProgressParamsValue::WorkDone(work) = params.value;
        let state = match work {
            WorkDoneProgress::Begin(begin) => ProgressState::Begin {
                title: begin.title,
                message: begin.message,
                percentage: begin.percentage,
            },
            WorkDoneProgress::Report(report) => ProgressState::Report {
                message: report.message,
                percentage: report.percentage,
            },
            WorkDoneProgress::End(end) => ProgressState::End {
                message: end.message,
            },
        };
        Self { token, state }
    }
}
