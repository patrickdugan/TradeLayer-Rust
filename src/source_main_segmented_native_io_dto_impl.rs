/// Independently produced native IO representation. Never a StorageStopped or
/// validation-result input; only reconstructible actual std::io errors admit.
pub struct ActualValidityIoDataV1 {
    kind: std::io::ErrorKind,
    raw_os: Option<i32>,
    message: String,
}
fn validity_io_kind(text: &str) -> Result<std::io::ErrorKind> {
    use std::io::ErrorKind as K;
    Ok(match text {
        "NotFound" => K::NotFound,
        "PermissionDenied" => K::PermissionDenied,
        "AlreadyExists" => K::AlreadyExists,
        "InvalidInput" => K::InvalidInput,
        "InvalidData" => K::InvalidData,
        "TimedOut" => K::TimedOut,
        "WriteZero" => K::WriteZero,
        "Interrupted" => K::Interrupted,
        "UnexpectedEof" => K::UnexpectedEof,
        "Unsupported" => K::Unsupported,
        "OutOfMemory" => K::OutOfMemory,
        "WouldBlock" => K::WouldBlock,
        "NotADirectory" => K::NotADirectory,
        "IsADirectory" => K::IsADirectory,
        "DirectoryNotEmpty" => K::DirectoryNotEmpty,
        "ReadOnlyFilesystem" => K::ReadOnlyFilesystem,
        "StorageFull" => K::StorageFull,
        "NotSeekable" => K::NotSeekable,
        "QuotaExceeded" => K::QuotaExceeded,
        "FileTooLarge" => K::FileTooLarge,
        "ResourceBusy" => K::ResourceBusy,
        "ExecutableFileBusy" => K::ExecutableFileBusy,
        "Deadlock" => K::Deadlock,
        "CrossesDevices" => K::CrossesDevices,
        "TooManyLinks" => K::TooManyLinks,
        "InvalidFilename" => K::InvalidFilename,
        "ArgumentListTooLong" => K::ArgumentListTooLong,
        "ConnectionRefused" => K::ConnectionRefused,
        "ConnectionReset" => K::ConnectionReset,
        "HostUnreachable" => K::HostUnreachable,
        "NetworkUnreachable" => K::NetworkUnreachable,
        "ConnectionAborted" => K::ConnectionAborted,
        "NotConnected" => K::NotConnected,
        "AddrInUse" => K::AddrInUse,
        "AddrNotAvailable" => K::AddrNotAvailable,
        "NetworkDown" => K::NetworkDown,
        "BrokenPipe" => K::BrokenPipe,
        "Other" => K::Other,
        _ => return Err(ActiveServiceWalErrorV1::InvalidRecord),
    })
}
impl ActualValidityIoDataV1 {
    pub(crate) fn capture(error: &std::io::Error) -> Result<Self> {
        let value = Self {
            kind: error.kind(),
            raw_os: error.raw_os_error(),
            message: error.to_string(),
        };
        if value.raw_os.is_none() {
            validity_io_kind(&format!("{:?}", value.kind))?;
        }
        let replay = value.reconstruct();
        if format!("{replay:?}") != format!("{error:?}") {
            return Err(ActiveServiceWalErrorV1::InvalidRecord);
        }
        Ok(value)
    }
    pub fn reconstruct(&self) -> std::io::Error {
        self.raw_os.map_or_else(
            || std::io::Error::new(self.kind, self.message.clone()),
            std::io::Error::from_raw_os_error,
        )
    }
    pub fn value(&self) -> V {
        obj(&[
            ("kind", s(format!("{:?}", self.kind))),
            (
                "representation",
                s(if self.raw_os.is_some() {
                    "Os"
                } else {
                    "CustomString"
                }),
            ),
            (
                "rawOsError",
                self.raw_os.map_or(V::Null, |v| s(v.to_string())),
            ),
            ("message", s(&self.message)),
        ])
    }
    pub fn from_value(value: &V) -> Result<Self> {
        actual_fact_exact_keys(value, &["kind", "representation", "rawOsError", "message"])?;
        let kind_text = history_text(&value.get("kind"))?.to_owned();
        let raw_os = if value.get("rawOsError") == V::Null {
            None
        } else {
            let text = history_text(&value.get("rawOsError"))?.to_owned();
            let n = text
                .parse::<i32>()
                .map_err(|_| ActiveServiceWalErrorV1::InvalidRecord)?;
            if n.to_string() != text {
                return Err(ActiveServiceWalErrorV1::InvalidRecord);
            }
            Some(n)
        };
        if value.get("representation")
            != s(if raw_os.is_some() {
                "Os"
            } else {
                "CustomString"
            })
        {
            return Err(ActiveServiceWalErrorV1::InvalidRecord);
        }
        let kind = match raw_os {
            Some(code) => {
                let kind = std::io::Error::from_raw_os_error(code).kind();
                if format!("{kind:?}") != kind_text {
                    return Err(ActiveServiceWalErrorV1::WrongContext);
                }
                kind
            }
            None => validity_io_kind(&kind_text)?,
        };
        let data = Self {
            kind,
            raw_os,
            message: history_text(&value.get("message"))?.into(),
        };
        let error = data.reconstruct();
        if error.kind() != kind || error.to_string() != data.message {
            return Err(ActiveServiceWalErrorV1::WrongContext);
        }
        Ok(data)
    }
}
