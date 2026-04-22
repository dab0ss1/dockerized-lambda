use std::fmt;

#[derive(Debug, Clone)]
pub enum CustomHeader {
    FunctionName,
    LambdaPort,
    ErrorType,
    HandlerVersion,
    ProcessingDelayMs,
    LambdaRequestId,
    LambdaExecutionTimeMs,
}

impl AsRef<str> for CustomHeader {
    fn as_ref(&self) -> &str {
        match self {
            CustomHeader::FunctionName => "x-function-name",
            CustomHeader::LambdaPort => "x-lambda-port",
            CustomHeader::ErrorType => "x-error-type",
            CustomHeader::HandlerVersion => "x-handler-version",
            CustomHeader::ProcessingDelayMs => "x-processing-delay-ms",
            CustomHeader::LambdaRequestId => "x-lambda-request-id",
            CustomHeader::LambdaExecutionTimeMs => "x-lambda-execution-time-ms",
        }
    }
}

impl fmt::Display for CustomHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}