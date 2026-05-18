use std::fmt::Display;

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    NoError = 0x0,
    ProtocolError = 0x1,
    InternalError = 0x2,
    FlowControlError = 0x3,
    SettingsTimeout = 0x4,
    StreamClosed = 0x5,
    FrameSizeError = 0x6,
    RefusedStream = 0x7,
    Cancel = 0x8,
    CompressionError = 0x9,
    ConnectError = 0xa,
    EnhanceYourCalm = 0xb,
    InadequateSecurity = 0xc,
    HTTP11Required = 0xd,
}

impl Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NoError => "No error (0x0)",
            Self::ProtocolError => "Protocol error (0x1)",
            Self::InternalError => "Internal error (0x2)",
            Self::FlowControlError => "Flow control error (0x3)",
            Self::SettingsTimeout => "Settings timeout (0x4)",
            Self::StreamClosed => "Stream closed (0x5)",
            Self::FrameSizeError => "Frame size error (0x6)",
            Self::RefusedStream => "Refused stream (0x7)",
            Self::Cancel => "Cancel (0x8)",
            Self::CompressionError => "Compression error (0x9)",
            Self::ConnectError => "Connect error (0xa)",
            Self::EnhanceYourCalm => "Enhance your calm (0xb)",
            Self::InadequateSecurity => "Inadequate security (0xc)",
            Self::HTTP11Required => "HTTP/1.1 required (0xd)",
        })
    }
}

impl TryFrom<u32> for ErrorCode {
    type Error = ();

    fn try_from(value: u32) -> std::result::Result<Self, Self::Error> {
        match value {
            0x0 => Ok(Self::NoError),
            0x1 => Ok(Self::ProtocolError),
            0x2 => Ok(Self::InternalError),
            0x3 => Ok(Self::FlowControlError),
            0x4 => Ok(Self::SettingsTimeout),
            0x5 => Ok(Self::StreamClosed),
            0x6 => Ok(Self::FrameSizeError),
            0x7 => Ok(Self::RefusedStream),
            0x8 => Ok(Self::Cancel),
            0x9 => Ok(Self::CompressionError),
            0xa => Ok(Self::ConnectError),
            0xb => Ok(Self::EnhanceYourCalm),
            0xc => Ok(Self::InadequateSecurity),
            0xd => Ok(Self::HTTP11Required),
            _ => Err(()),
        }
    }
}

pub type Result<T> = std::result::Result<T, ErrorCode>;

#[derive(Debug)]
pub enum IOProtoError {
    Io(std::io::Error),
    Protocol(ErrorCode),
}

impl From<std::io::Error> for IOProtoError {
    fn from(value: std::io::Error) -> Self {
        IOProtoError::Io(value)
    }
}

impl From<ErrorCode> for IOProtoError {
    fn from(value: ErrorCode) -> Self {
        IOProtoError::Protocol(value)
    }
}

impl Display for IOProtoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "Failed IO operation: {e}"),
            Self::Protocol(e) => write!(f, "Protocol Error: {e}"),
        }
    }
}

impl IOProtoError {
    pub fn get_error_code(&self) -> ErrorCode {
        match self {
            &Self::Protocol(c) => c,
            Self::Io(_) => ErrorCode::InternalError,
        }
    }
}

pub type IOProtoResult<T> = std::result::Result<T, IOProtoError>;
