use fern::{
    colors::{Color, ColoredLevelConfig},
    Dispatch,
};
use log::error;
use syslog::Formatter3164;

use crate::youtube::body_to_string;

/// Creates a syslog dispatcher for sending messages to the syslog
pub fn create_syslog_dispatcher<'a>(
    colors_line: ColoredLevelConfig,
    syslog_formatter: &Formatter3164,
) -> Dispatch {
    return fern::Dispatch::new()
        .level(log::LevelFilter::Info)
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{}{}\x1B[0m",
                format_args!(
                    "\x1B[{}m",
                    colors_line.get_color(&record.level()).to_fg_str()
                ),
                message
            ));
        })
        .chain(syslog::unix(syslog_formatter.clone()).unwrap());
}

/// Sets up regular logging
pub fn setup_log(verbose: bool) {
    let syslog_formatter = syslog::Formatter3164 {
        facility: syslog::Facility::LOG_USER,
        hostname: None,
        process: "youtubeservice".to_owned(),
        pid: 0,
    };
    let colors_line = ColoredLevelConfig::new()
        .error(Color::Red)
        .warn(Color::Yellow)
        .info(Color::Green)
        .debug(Color::White)
        .trace(Color::BrightBlack);
    let colors_level = colors_line.clone().info(Color::Green);

    let syslog_dispatcher: Dispatch;
    if let Some(server) = std::env::var_os("SYSLOG_SERVER") {
        if let Ok(tcp) = syslog::tcp(syslog_formatter.clone(), server.into_string().unwrap()) {
            syslog_dispatcher = create_syslog_dispatcher(colors_line, &syslog_formatter).chain(tcp);
        } else {
            syslog_dispatcher = create_syslog_dispatcher(colors_line, &syslog_formatter);
        }
    } else {
        syslog_dispatcher = create_syslog_dispatcher(colors_line, &syslog_formatter);
    }

    fern::Dispatch::new()
        .chain(
            fern::Dispatch::new()
                .level(if verbose {
                    log::LevelFilter::Debug
                } else {
                    log::LevelFilter::Info
                })
                .format(move |out, message, record| {
                    out.finish(format_args!(
                        "{color_line}[{date}][{target}][{level}{color_line}] {message}\x1B[0m",
                        color_line = format_args!(
                            "\x1B[{}m",
                            colors_line.get_color(&record.level()).to_fg_str()
                        ),
                        date = chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                        target = record.target(),
                        level = colors_level.color(record.level()),
                        message = message,
                    ));
                })
                .chain(std::io::stdout()),
        )
        .chain(syslog_dispatcher)
        .apply()
        .unwrap();
}

/// Handles YouTube errors
pub async fn log_google_errors(error: google_youtube3::Error) -> String {
    match error {
        google_youtube3::Error::BadRequest(bad_request) => {
            let message = format!("BadRequest: {}", bad_request.error.message);
            error!("{}", message);
            return message;
        }
        google_youtube3::Error::Failure(failure) => {
            let body_string = body_to_string(failure).await;
            let message = format!("Failure: {}", body_string);
            error!("{}", message);
            return message;
        }
        google_youtube3::Error::FieldClash(field_clash) => {
            let message = format!("FieldClash: {}", field_clash);
            error!("{}", message);
            return message;
        }
        google_youtube3::Error::HttpError(http_error) => {
            let message = format!("HttpError: {}", http_error);
            error!("{}", message);
            return message;
        }
        google_youtube3::Error::Io(io_error) => {
            let message = format!("IOError: {}", io_error);
            error!("{}", message);
            return message;
        }
        google_youtube3::Error::JsonDecodeError(body, json_error) => {
            let message = format!("JsonDecodeError: {}, body: {}", json_error, body);
            error!("{}", message);
            return message;
        }
        google_youtube3::Error::MissingToken(missing_token) => {
            let message = format!("MissingToken: {}", missing_token);
            error!("{}", message);
            return message;
        }
        google_youtube3::Error::UploadSizeLimitExceeded(uploaded_size, max_size) => {
            let message = format!(
                "UploadSizeLimitExceeded: uploaded_size: {}, max_size: {}",
                uploaded_size, max_size
            );
            error!("{}", message);
            return message;
        }
        google_youtube3::Error::MissingAPIKey => {
            let message = format!("MissingAPIKey");
            error!("{}", message);
            return message;
        }
        google_youtube3::Error::Cancelled => {
            let message = format!("Cancelled");
            error!("{}", message);
            return message;
        }
    }
}
