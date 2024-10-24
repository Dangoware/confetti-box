use std::error::Error;

use chrono::TimeDelta;

pub fn parse_time_string(string: &str) -> Result<TimeDelta, Box<dyn Error>> {
    if string.len() > 7 {
        return Err("Not valid time string".into())
    }

    let unit = string.chars().last();
    let multiplier = if let Some(u) = unit {
        if !u.is_ascii_alphabetic() {
            return Err("Not valid time string".into())
        }

        match u {
            'D' | 'd' => TimeDelta::days(1),
            'H' | 'h' => TimeDelta::hours(1),
            'M' | 'm' => TimeDelta::minutes(1),
            'S' | 's' => TimeDelta::seconds(1),
            _ => return Err("Not valid time string".into()),
        }
    } else {
        return Err("Not valid time string".into())
    };

    let time = if let Ok(n) = string[..string.len() - 1].parse::<i32>() {
        n
    } else {
        return Err("Not valid time string".into())
    };

    let final_time = multiplier * time;

    Ok(final_time)
}

pub fn to_pretty_time(seconds: u32) -> String {
    let days = (seconds as f32 / 86400.0).floor();
    let hour = ((seconds as f32 - (days as f32 * 86400.0)) / 3600.0).floor();
    let mins = ((seconds as f32 - (hour * 3600.0) - (days * 86400.0)) / 60.0).floor();
    let secs = seconds as f32 - (hour as f32 * 3600.0) - (mins as f32 * 60.0) - (days as f32 * 86400.0);

    let days = if days == 0.0 {"".to_string()} else if days == 1.0 {days.to_string() + "<br>day"} else {days.to_string() + "<br>days"};
    let hour = if hour == 0.0 {"".to_string()} else if hour == 1.0 {hour.to_string() + "<br>hour"} else {hour.to_string() + "<br>hours"};
    let mins = if mins == 0.0 {"".to_string()} else if mins == 1.0 {mins.to_string() + "<br>minute"} else {mins.to_string() + "<br>minutes"};
    let secs = if secs == 0.0 {"".to_string()} else if secs == 1.0 {secs.to_string() + "<br>second"} else {secs.to_string() + "<br>seconds"};

    (days + " " + &hour + " " + &mins + " " + &secs).trim().to_string()
}
