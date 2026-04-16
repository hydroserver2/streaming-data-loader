use std::{fs, path::PathBuf};

use chrono::{NaiveDate, NaiveDateTime};
use csv::ReaderBuilder;

use crate::models::CsvPreviewResponse;

const DELIMITER_CANDIDATES: [char; 5] = [',', '\t', ';', '|', ' '];

pub fn preview_csv(path: &str, rows: usize) -> Result<CsvPreviewResponse, String> {
    let file_path = expand_path(path)?;
    if !file_path.exists() {
        return Err("Can't find the data file. It may have been moved or renamed.".to_string());
    }

    let bytes = fs::read(&file_path).map_err(|err| err.to_string())?;
    let (raw_text, encoding) = decode_text(&bytes)?;
    let raw_lines: Vec<String> = raw_text.lines().map(|line| line.to_string()).collect();
    let delimiter = detect_delimiter(raw_lines.iter().take(rows).map(String::as_str));
    let parsed_rows: Vec<Vec<String>> = raw_lines
        .iter()
        .take(rows)
        .filter(|line| !line.trim().is_empty())
        .map(|line| parse_line(line, delimiter))
        .collect();

    let header_index = detect_header_row(&parsed_rows);
    let data_start_index = detect_data_start_row(&parsed_rows, header_index);

    Ok(CsvPreviewResponse {
        raw_lines: raw_lines.into_iter().take(rows).collect(),
        parsed_rows: match header_index {
            Some(index) => parsed_rows.into_iter().skip(index).collect(),
            None => parsed_rows,
        },
        detected_header_row: header_index.map(|index| index as u32 + 1),
        detected_data_start_row: data_start_index.map(|index| index as u32 + 1),
        detected_delimiter: delimiter.to_string(),
        total_lines: raw_text.lines().count(),
        encoding,
    })
}

pub fn detect_delimiter<'a>(lines: impl Iterator<Item = &'a str>) -> char {
    let sampled_lines: Vec<&str> = lines.filter(|line| !line.trim().is_empty()).collect();
    let mut best_delimiter = ',';
    let mut best_score = -1_i64;

    for delimiter in DELIMITER_CANDIDATES {
        let counts: Vec<usize> = sampled_lines
            .iter()
            .map(|line| parse_line(line, delimiter).len())
            .collect();

        if counts.is_empty() {
            continue;
        }

        let mut frequency = std::collections::HashMap::<usize, usize>::new();
        for count in counts {
            *frequency.entry(count).or_insert(0) += 1;
        }

        if let Some((mode_count, occurrences)) = frequency
            .into_iter()
            .max_by_key(|(count, occurrences)| (*occurrences, *count))
        {
            let score = (occurrences * mode_count) as i64;
            if score > best_score {
                best_score = score;
                best_delimiter = delimiter;
            }
        }
    }

    best_delimiter
}

pub fn parse_line(line: &str, delimiter: char) -> Vec<String> {
    let mut reader = ReaderBuilder::new()
        .has_headers(false)
        .delimiter(delimiter as u8)
        .from_reader(line.as_bytes());

    reader
        .records()
        .next()
        .transpose()
        .ok()
        .flatten()
        .map(|record| record.iter().map(|value| value.to_string()).collect())
        .unwrap_or_else(|| vec![line.to_string()])
}

fn expand_path(path: &str) -> Result<PathBuf, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Choose a CSV file path.".to_string());
    }

    if let Some(stripped) = trimmed.strip_prefix("~/") {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .map_err(|_| "Couldn't resolve the home directory.".to_string())?;
        return Ok(PathBuf::from(home).join(stripped));
    }

    Ok(PathBuf::from(trimmed))
}

pub(crate) fn decode_text(bytes: &[u8]) -> Result<(String, String), String> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        let text = String::from_utf8(bytes[3..].to_vec())
            .map_err(|_| "Couldn't read the file encoding. Try exporting as UTF-8.".to_string())?;
        return Ok((text, "utf-8-sig".to_string()));
    }

    if let Ok(text) = String::from_utf8(bytes.to_vec()) {
        return Ok((text, "utf-8".to_string()));
    }

    let latin1 = bytes.iter().map(|byte| *byte as char).collect::<String>();
    Ok((latin1, "latin-1".to_string()))
}

fn detect_header_row(rows: &[Vec<String>]) -> Option<usize> {
    for (index, row) in rows.iter().enumerate() {
        let cleaned: Vec<&str> = row
            .iter()
            .map(|cell| cell.trim())
            .filter(|cell| !cell.is_empty())
            .collect();
        if cleaned.len() < 3 {
            continue;
        }

        if cleaned.iter().all(|cell| !looks_numeric_or_timestamp(cell)) {
            return Some(index);
        }
    }

    if rows.is_empty() {
        None
    } else {
        Some(0)
    }
}

fn detect_data_start_row(rows: &[Vec<String>], header_index: Option<usize>) -> Option<usize> {
    let header_index = header_index?;
    let expected_columns = rows.get(header_index).map(Vec::len).unwrap_or_default();

    for index in (header_index + 1)..rows.len() {
        let row: Vec<String> = rows[index]
            .iter()
            .map(|cell| cell.trim().to_string())
            .collect();
        if row.len() != expected_columns {
            continue;
        }

        let meaningful: Vec<&str> = row
            .iter()
            .map(String::as_str)
            .filter(|cell| !cell.is_empty())
            .collect();
        if meaningful.len() < 2 {
            continue;
        }

        let numeric_or_timestamp_count = meaningful
            .iter()
            .filter(|cell| looks_numeric_or_timestamp(cell))
            .count();

        if numeric_or_timestamp_count >= usize::max(2, meaningful.len() / 2) {
            return Some(index);
        }
    }

    None
}

fn looks_numeric_or_timestamp(value: &str) -> bool {
    if value.parse::<f64>().is_ok() {
        return true;
    }

    parse_preview_timestamp(value).is_some()
}

fn parse_preview_timestamp(value: &str) -> Option<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    for format in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%m/%d/%Y %H:%M"] {
        if NaiveDateTime::parse_from_str(trimmed, format).is_ok() {
            return Some(());
        }
    }

    if NaiveDate::parse_from_str(trimmed, "%Y-%m-%d").is_ok() {
        return Some(());
    }

    None
}
