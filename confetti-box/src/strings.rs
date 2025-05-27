use std::error::Error;

use chrono::TimeDelta;

pub fn parse_time_string(string: &str) -> Result<TimeDelta, Box<dyn Error>> {
    if string.len() > 7 {
        return Err("Not valid time string".into());
    }

    let unit = string.chars().last();
    let multiplier = if let Some(u) = unit {
        if !u.is_ascii_alphabetic() {
            return Err("Not valid time string".into());
        }

        match u {
            'D' | 'd' => TimeDelta::days(1),
            'H' | 'h' => TimeDelta::hours(1),
            'M' | 'm' => TimeDelta::minutes(1),
            'S' | 's' => TimeDelta::seconds(1),
            _ => return Err("Not valid time string".into()),
        }
    } else {
        return Err("Not valid time string".into());
    };

    let time = if let Ok(n) = string[..string.len() - 1].parse::<i32>() {
        n
    } else {
        return Err("Not valid time string".into());
    };

    let final_time = multiplier * time;

    Ok(final_time)
}

pub enum BreakStyle {
    Break,
    Newline,
    Space,
    Nothing,
}

pub enum TimeGranularity {
    Days,
    Hours,
    Minutes,
    Seconds,
}

pub fn pretty_time_short(seconds: i64) -> String {
    let days = (seconds as f32 / 86400.0).floor();
    let hour = ((seconds as f32 - (days * 86400.0)) / 3600.0).floor();
    let mins = ((seconds as f32 - (hour * 3600.0) - (days * 86400.0)) / 60.0).floor();
    let secs = seconds as f32 - (hour * 3600.0) - (mins * 60.0) - (days * 86400.0);

    let days = if days > 0. {days.to_string() + "d"} else { "".into() };
    let hour = if hour > 0. {hour.to_string() + "h"} else { "".into() };
    let mins = if mins > 0. {mins.to_string() + "m"} else { "".into() };
    let secs = if secs > 0. {secs.to_string() + "s"} else { "".into() };

    (days + " " + &hour + " " + &mins + " " + &secs)
    .trim()
    .to_string()
}

pub fn pretty_time(seconds: i64, breaks: BreakStyle, granularity: TimeGranularity) -> String {
    let days = (seconds as f32 / 86400.0).floor();
    let hour = ((seconds as f32 - (days * 86400.0)) / 3600.0).floor();
    let mins = ((seconds as f32 - (hour * 3600.0) - (days * 86400.0)) / 60.0).floor();
    let secs = seconds as f32 - (hour * 3600.0) - (mins * 60.0) - (days * 86400.0);

    let days = if days == 0.0 {
        "".to_string()
    } else if days == 1.0 {
        days.to_string() + "\nday"
    } else {
        days.to_string() + "\ndays"
    };

    let hour = if hour == 0.0 {
        "".to_string()
    } else if hour == 1.0 {
        hour.to_string() + "\nhour"
    } else {
        hour.to_string() + "\nhours"
    };

    let mins = if mins == 0.0 {
        "".to_string()
    } else if mins == 1.0 {
        mins.to_string() + "\nminute"
    } else {
        mins.to_string() + "\nminutes"
    };

    let secs = if secs == 0.0 {
        "".to_string()
    } else if secs == 1.0 {
        secs.to_string() + "\nsecond"
    } else {
        secs.to_string() + "\nseconds"
    };

    let mut out_string = match granularity {
        TimeGranularity::Days => days,
        TimeGranularity::Hours => days + " " + &hour,
        TimeGranularity::Minutes => days + " " + &hour + " " + &mins,
        TimeGranularity::Seconds => days + " " + &hour + " " + &mins + " " + &secs,
    }.trim().to_string();

    match breaks {
        BreakStyle::Break => out_string = out_string.replace("\n", "<br>"),
        BreakStyle::Newline => (),
        BreakStyle::Space => out_string = out_string.replace("\n", " "),
        BreakStyle::Nothing => out_string = out_string.replace("\n", ""),
    }

    out_string
}

pub fn to_pretty_size(size: u64) -> String {
    if size < 1000 {
        size.to_string() + " B"
    } else if size < 1000u64.pow(2) {
        (size / 1000).to_string() + " kB"
    } else if size < 1000u64.pow(3) {
        (size / 1000u64.pow(2)).to_string() + " MB"
    } else if size < 1000u64.pow(4) {
        (size / 1000u64.pow(3)).to_string() + " GB"
    } else if size < 1000u64.pow(5) {
        (size as u128 / 1000u128.pow(4)).to_string() + " TB"
    } else if size < 1000u64.pow(6) {
        (size as u128 / 1000u128.pow(5)).to_string() + " PB"
    } else if size < 1000u64.pow(7) {
        (size as u128 / 1000u128.pow(6)).to_string() + " EB"
    } else {
        size.to_string() + " B"
    }
}
