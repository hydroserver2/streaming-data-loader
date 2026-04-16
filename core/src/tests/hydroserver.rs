use super::*;
use chrono::TimeZone;

#[test]
fn observation_batch_payload_matches_sensorthings_schema() {
    let observations = vec![
        ObservationPayloadRow {
            phenomenon_time: Utc.with_ymd_and_hms(2026, 4, 3, 8, 0, 0).unwrap(),
            result: json!(2.41),
        },
        ObservationPayloadRow {
            phenomenon_time: Utc.with_ymd_and_hms(2026, 4, 3, 8, 5, 0).unwrap(),
            result: json!("qualitative"),
        },
    ];

    let body = json!({
        "fields": ["phenomenonTime", "result"],
        "data": observations
            .iter()
            .map(|row| json!([row.phenomenon_time.to_rfc3339(), row.result]))
            .collect::<Vec<_>>(),
    });

    assert_eq!(
        body["fields"],
        json!(["phenomenonTime", "result"]),
        "fields must use SensorThings naming"
    );

    let data = body["data"].as_array().expect("data should be array");
    assert_eq!(data.len(), 2);
    assert_eq!(data[0][0], "2026-04-03T08:00:00+00:00");
    assert_eq!(data[0][1], 2.41);
    assert_eq!(data[1][1], "qualitative");
}

#[test]
fn build_url_normalizes_correctly() {
    assert_eq!(
        build_url("https://example.com/", "/api/data/test"),
        "https://example.com/api/data/test"
    );
    assert_eq!(
        build_url("https://example.com", "api/data/test"),
        "https://example.com/api/data/test"
    );
}

#[test]
fn request_error_retryable_classification() {
    assert!(RequestError::Connection.is_retryable());
    assert!(RequestError::Timeout.is_retryable());
    assert!(RequestError::Http {
        status: Some(StatusCode::INTERNAL_SERVER_ERROR),
        message: "error".to_string()
    }
    .is_retryable());
    assert!(RequestError::Http {
        status: Some(StatusCode::BAD_GATEWAY),
        message: "error".to_string()
    }
    .is_retryable());

    assert!(!RequestError::Http {
        status: Some(StatusCode::BAD_REQUEST),
        message: "error".to_string()
    }
    .is_retryable());
    assert!(!RequestError::Http {
        status: Some(StatusCode::UNPROCESSABLE_ENTITY),
        message: "error".to_string()
    }
    .is_retryable());
    assert!(!RequestError::Other("misc".to_string()).is_retryable());
}
