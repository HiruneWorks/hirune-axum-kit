use crate::http::error::ApiError;
use application::files::FileSummary;
use axum::http::{HeaderMap, HeaderValue, header};
use std::ops::Range;

pub(super) fn original_headers(
    headers: &mut HeaderMap,
    file: &FileSummary,
    byte_length: u64,
) -> Result<(), ApiError> {
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(header::CONTENT_LENGTH, value(&byte_length.to_string())?);
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    let filename =
        percent_encoding::utf8_percent_encode(&file.filename, percent_encoding::NON_ALPHANUMERIC);
    headers.insert(
        header::CONTENT_DISPOSITION,
        value(&format!(
            "attachment; filename=\"attachment\"; filename*=UTF-8''{filename}"
        ))?,
    );
    if let Some(hash) = &file.content_hash {
        headers.insert(header::ETAG, value(&format!("\"{hash}\""))?);
    }
    Ok(())
}

pub(super) fn value(value: &str) -> Result<HeaderValue, ApiError> {
    HeaderValue::from_str(value).map_err(|_| {
        application::ApplicationError::internal(anyhow::anyhow!("invalid file response header"))
            .into()
    })
}

pub(super) fn parse_range_header(
    headers: &HeaderMap,
    size: u64,
    etag: Option<&str>,
) -> Result<Option<Range<u64>>, ()> {
    let mut ranges = headers.get_all(header::RANGE).iter();
    let Some(range) = ranges.next() else {
        return Ok(None);
    };
    if ranges.next().is_some() {
        return Err(());
    }
    if let Some(if_range) = headers.get(header::IF_RANGE)
        && (etag.is_none() || if_range.to_str().ok() != etag)
    {
        return Ok(None);
    }
    let range = range
        .to_str()
        .map_err(|_| ())?
        .strip_prefix("bytes=")
        .ok_or(())?;
    if range.contains(',') || size == 0 {
        return Err(());
    }
    let (start, end) = range.split_once('-').ok_or(())?;
    let number = |value: &str| -> Result<u64, ()> {
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(());
        }
        value.parse().map_err(|_| ())
    };
    let range = if start.is_empty() {
        let suffix = number(end)?;
        if suffix == 0 {
            return Err(());
        }
        size.saturating_sub(suffix)..size
    } else {
        let start = number(start)?;
        let end = if end.is_empty() {
            size
        } else {
            number(end)?.saturating_add(1).min(size)
        };
        if start >= end || start >= size {
            return Err(());
        }
        start..end
    };
    Ok(Some(range))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn byte_ranges_cover_suffix_resume_and_unsatisfiable_requests() {
        for (input, expected) in [
            ("bytes=2-5", Some(2..6)),
            ("bytes=2-", Some(2..10)),
            ("bytes=-3", Some(7..10)),
            ("bytes=-99", Some(0..10)),
        ] {
            let headers = [(header::RANGE, value(input).unwrap())]
                .into_iter()
                .collect();
            assert_eq!(parse_range_header(&headers, 10, None), Ok(expected));
        }
        for input in [
            "bytes=10-",
            "bytes=3-2",
            "bytes=-0",
            "bytes=0-1,3-4",
            "bytes=+1-2",
            "items=0-1",
        ] {
            let headers = [(header::RANGE, value(input).unwrap())]
                .into_iter()
                .collect();
            assert!(parse_range_header(&headers, 10, None).is_err());
        }
        let headers = [
            (header::RANGE, value("bytes=0-1").unwrap()),
            (header::IF_RANGE, value("\"old\"").unwrap()),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            parse_range_header(&headers, 10, Some("\"current\"")),
            Ok(None)
        );
    }
}
