#![allow(incomplete_features)]
#![allow(dead_code)]

#![feature(generic_const_exprs)]
#![feature(let_chains)]

pub mod data;
pub mod serde_structs;

use crate::data::{dataset_average, dataset_sum, Graph, TimeQuantity};
use crate::serde_structs::{Call, DirectMessages, Message, UninitDirectMessages};
use anyhow::{Context, Result};
use chrono::{Datelike, Days, NaiveDate, TimeDelta, Timelike, Weekday};
use crossterm::cursor::{MoveTo, MoveToNextLine};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::style::{Color, Colors, Print, SetColors};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType};
use crossterm::{event, execute};
use fxhash::FxHashMap;
use image::{ImageFormat, Pixel, Rgba};
use itertools::Itertools;
use num_format::{Locale, ToFormattedString};
use num_traits::{FromPrimitive, Pow};
use std::fmt::Write;
use std::fs::File;
use std::io::stdout;
use std::path::Path;
use std::time::Instant;
use clipboard_rs::Clipboard;

fn main() -> Result<()> {
    let Some(path) = std::env::args().nth(1) else {
        println!("No path specified; usage: discord_dm_analyser <file>");
        std::process::exit(0);
    };

    parse_dms(&path).context("Failed to evalulate DM information")
}

fn parse_dms<P: AsRef<Path>>(path: P) -> Result<()> {
    println!("Parsing DMs...");
    let start = Instant::now();
    let dms: DirectMessages = serde_json::from_slice::<UninitDirectMessages>(&std::fs::read(path)?)?.try_into()?;
    println!("Parsed DMs in {}", TimeQuantity::from(start.elapsed().as_millis() as usize));

    enable_raw_mode()?;
    let selections = select_data_calculations()?;
    disable_raw_mode()?;

    let mut buf = String::new();

    for selection in selections {
        write!(&mut buf, "{}", selection(&dms)?)?;
    }

    println!("{buf}");
    clipboard_rs::ClipboardContext::new().ok().context("Could not create clipboard")?.set_text(buf.clone()).ok().context("Failed to set clipboard content")?;
    println!("Copied to clipboard!");
    std::fs::write("discord_dm_analysis.txt", buf)?;
    println!("Written to 'discord_dm_analysis.txt'!");

    loop {}
}

fn select_data_calculations() -> Result<Vec<fn(&DirectMessages) -> Result<String>>> {
    enum SelectionInput {
        Finish,
        Toggle,
        Up,
        Down
    }

    fn read_valid_input() -> SelectionInput {
        use SelectionInput::*;

        loop {
            let first_byte = event::read();
            match first_byte {
                Ok(Event::Key(KeyEvent { kind: KeyEventKind::Press, code: KeyCode::Enter, .. })) => return Finish,
                Ok(Event::Key(KeyEvent { kind: KeyEventKind::Press, code: KeyCode::Up, .. })) => return Up,
                Ok(Event::Key(KeyEvent { kind: KeyEventKind::Press, code: KeyCode::Down, .. })) => return Down,
                Ok(Event::Key(KeyEvent { kind: KeyEventKind::Press, code: KeyCode::Left | KeyCode::Right, .. })) => return Toggle,
                Ok(Event::Key(KeyEvent { kind: KeyEventKind::Press, code: KeyCode::Char('c'), modifiers: KeyModifiers::CONTROL, .. })) => {
                    let _ = disable_raw_mode();
                    std::process::exit(1)
                },
                _ => {},
            }
        }
    }

    fn display_line(name: &str, toggled: bool, selected: bool) -> Result<()> {
        // < Top Call Lengths - DISABLED >
        let plain_text_color = if selected { Colors::new(Color::Black, Color::White) } else { Colors::new(Color::White, Color::Black)};
        let toggle_color = match (toggled, selected) {
            (false, false) => Colors::new(Color::Red, Color::Black),
            (false, true) => Colors::new(Color::Red, Color::White),
            (true, false) => Colors::new(Color::Green, Color::Black),
            (true, true) => Colors::new(Color::Green, Color::White),
        };

        execute!(
            stdout(),
            SetColors(plain_text_color),
            Print("< "),
            Print(name),
            Print(" - "),
            SetColors(toggle_color),
            Print(if toggled { "ENABLED" } else { "DISABLED" }),
            SetColors(plain_text_color),
            Print(" > "),
            SetColors(Colors::new(Color::Reset, Color::Reset)),
            MoveToNextLine(1),
        )?;

        Ok(())
    }

    const SELECTIONS: &[(&'static str, fn(&DirectMessages) -> Result<String>)] = &[
        ("First Message", first_message),
        ("Texting Frequency (Lifetime Graph; Weekly Buckets)", texting_frequency),
        ("Top Call Lengths", top_call_lengths),
        ("Total Call Lengths", total_call_lengths),
        ("Longest Time Between Messages", longest_time_between_messages),
        ("Longest Time Between Messages from Different Users", longest_time_between_different_users),
        ("100 Most Said Words", most_said_words),
        ("Words and Characters Written", words_and_characters_written),
        ("Most Characters Said in a Day", most_characters_said_in_a_day),
        ("Call Start Frequency (Time of Day Graph)", call_start_time_of_day_graph),
        ("Text Frequency (Time of Day Graph)", text_time_of_day_graph),
        ("Call Duration Graph (Annual Graph; Monthly Buckets)", call_duration_by_month_graph),
        ("Call Duration Graph (Weekly Graph; Daily Buckets)", call_duration_by_day_of_week_graph),
        ("Call Duration Graph (Daily Graph)", call_graph),
        ("Call Duration Graph PNG Export (Daily Graph)", call_png),
        ("Capitalization Rates (Annual Buckets)", capitalization_rates),
        ("Edited Rates (Annual Buckets)", edit_rates),
    ];


    execute!(stdout(), Clear(ClearType::All), MoveTo(0, 0))?;

    let mut selected = [false; const { SELECTIONS.len() }];
    let mut selected_line = 0_usize;

    for (idx, name, selected) in (0..SELECTIONS.len()).map(|idx| (idx, SELECTIONS[idx].0, selected[idx])) {
        display_line(name, selected, selected_line == idx)?;
    }

    loop {
        execute!(stdout(), MoveTo(0, selected_line as u16))?;
        match read_valid_input() {
            SelectionInput::Finish => {
                execute!(stdout(), MoveTo(0, SELECTIONS.len() as u16))?;
                return Ok((0..SELECTIONS.len()).filter(|&idx| selected[idx]).map(|idx| SELECTIONS[idx].1).collect::<Vec<_>>())
            },
            SelectionInput::Toggle => {
                selected[selected_line] = !selected[selected_line];
                display_line(SELECTIONS[selected_line].0, selected[selected_line], true)?;
            },
            SelectionInput::Up => {
                display_line(SELECTIONS[selected_line].0, selected[selected_line], false)?;
                selected_line = (selected_line + SELECTIONS.len() - 1) % SELECTIONS.len();
                execute!(stdout(), MoveTo(0, selected_line as u16))?;
                display_line(SELECTIONS[selected_line].0, selected[selected_line], true)?;
            },
            SelectionInput::Down => {
                display_line(SELECTIONS[selected_line].0, selected[selected_line], false)?;
                selected_line = (selected_line + 1) % SELECTIONS.len();
                execute!(stdout(), MoveTo(0, selected_line as u16))?;
                display_line(SELECTIONS[selected_line].0, selected[selected_line], true)?;
            },
        }
    }
}

pub fn generate_progress_bar<T, S: Fn(&T) -> usize>(width: usize, full_char: char, empty_char: char, max: usize, quantities: &[T], sum: S) -> String {
    let mut current_quantity = 0;
    let mut buf = String::with_capacity(width + 2);
    let _ = buf.write_char('[');
    for (idx, quantity) in quantities.iter().enumerate() {
        let quantity = sum(quantity);
        let _ = write!(&mut buf, "\x1B[{color}m", color = 92 + idx);
        let _ = buf.write_str(&full_char.to_string().repeat(width * quantity / max));
        current_quantity += width * quantity / max;
        let _ = write!(&mut buf, "​\x1B[0m");
    }
    let _ = buf.write_str(&empty_char.to_string().repeat(width.saturating_sub(current_quantity)));
    let _ = buf.write_char(']');
    buf
}

pub fn nth(n: usize) -> String {
    let mut buf = String::with_capacity(n.checked_ilog10().map_or(1, |x| x + 1) as usize + 2);
    let _ = write!(&mut buf, "{n}");
    if n / 10 % 10 == 1 {
        buf.push_str("th");
    } else {
        match n % 10 {
            1 => buf.push_str("st"),
            2 => buf.push_str("nd"),
            3 => buf.push_str("rd"),
            _ => buf.push_str("th"),
        }
    }
    buf
}

pub fn standard_deviation(sum: usize, iter: impl IntoIterator<Item=usize>, len: usize) -> f64 {
    let mut accumulated = 0_u128;
    for element in iter.into_iter() {
        accumulated += (len as i128 * element as i128 - sum as i128).pow(2) as u128;
    }
    (len as f64).pow(-1.5) * f64::sqrt(accumulated as f64)
}

fn first_message(dms: &DirectMessages) -> Result<String> {
    let mut buf = String::new();

    writeln!(&mut buf, "\n# First Message")?;
    let mut first_messages = vec![None; dms.channel.authors.len()];

    for text in dms.messages.iter().filter_map(Message::as_text_message) {
        let author_idx = dms.channel.authors.iter().position(|author| *author == text.author.name).unwrap();
        if first_messages[author_idx].is_none() {
            first_messages[author_idx] = Some(&text.content);
        }
    }

    for (author_idx, first_message) in first_messages.into_iter().enumerate() {
        let author = dms.channel.authors[author_idx];
        if let Some(first_message) = first_message {
            writeln!(&mut buf, "{author} = {first_message}")?;
        } else {
            writeln!(&mut buf, "{author} has no messages")?;
        }
    }

    Ok(buf)
}

fn texting_frequency(dms: &DirectMessages) -> Result<String> {
    let mut buf = String::new();

    writeln!(&mut buf, "\n#Texting Frequency (Lifetime Graph; Weekly Buckets)")?;

    let earliest_message_timestamp = dms.messages.iter().filter_map(Message::as_text_message).map(|text| text.timestamp).min().context("Expected a message")?;
    let earliest_message_date = NaiveDate::from_yo_opt(earliest_message_timestamp.year(), earliest_message_timestamp.ordinal0() / 7 * 7 + 1).unwrap();

    let mut graph = Graph::new(dms.channel.authors.clone(), 0, |idx| earliest_message_date.checked_add_days(Days::new(idx as u64 * 7)).unwrap().format("Week of %b %d, %Y").to_string(), dataset_sum, 50);

    for text in dms.messages.iter().filter_map(Message::as_text_message) {
        let date = text.timestamp.date();
        let delta = date - earliest_message_date;
        let idx = delta.num_days() as usize / 7;
        graph.add(text.author.name.as_str(), idx, 1);
    }

    writeln!(&mut buf, "{graph}")?;

    Ok(buf)
}

fn top_call_lengths(dms: &DirectMessages) -> Result<String> {
    let mut buf = String::new();

    writeln!(&mut buf, "\n# Top 25 Call Lengths")?;
    let mut lengths = dms.messages
        .iter()
        .filter_map(Message::as_call)
        .map(Call::duration)
        .collect::<Vec<_>>();

    lengths.sort();

    writeln!(&mut buf, "total calls: {}", lengths.len())?;
    writeln!(&mut buf, "8 hour calls: {}", lengths.iter().filter(|&&delta| delta >= TimeDelta::hours(8)).count())?;

    for (idx, duration) in lengths.into_iter().rev().take(25).enumerate() {
        let len = TimeQuantity::from(duration);
        writeln!(&mut buf, "{n}: length = {len:?}", n = idx + 1)?;
    }

    Ok(buf)
}

fn total_call_lengths(dms: &DirectMessages) -> Result<String> {
    let mut buf = String::new();

    writeln!(&mut buf, "\n# Total Call Lengths")?;
    let len = TimeQuantity::from(dms.messages
        .iter()
        .filter_map(Message::as_call)
        .map(Call::duration)
        .sum::<TimeDelta>());

    writeln!(&mut buf, "total length = {len}")?;

    Ok(buf)
}

fn longest_time_between_messages(dms: &DirectMessages) -> Result<String> {
    let mut buf = String::new();

    writeln!(&mut buf, "\n# Longest Time Between Messages")?;
    let mut differences = Vec::new();

    for (a, b) in dms.messages.iter().filter_map(Message::as_text_message).tuple_windows() {
        differences.push((b.timestamp - a.timestamp, a.content.as_str(), a.author.name.as_str(), a.id, b.id, a.timestamp, b.timestamp));
    }

    differences.sort_by_key(|(a, _, _, _, _, _, _)| *a);

    for (idx, (diff, content, author, first_id, second_id, first_timestamp, second_timestamp)) in differences.into_iter().rev().take(25).enumerate() {
        let difference = TimeQuantity::from(diff.num_milliseconds() as usize);
        writeln!(&mut buf, "{n}: diff = {difference}, first_timestamp = {first_timestamp}, second_timestamp = {second_timestamp}, first_id = {first_id}, second_id = {second_id} | content = {content:?}, author = {author}", n = idx + 1)?;
    }

    Ok(buf)
}

fn longest_time_between_different_users(dms: &DirectMessages) -> Result<String> {
    let mut buf = String::new();

    writeln!(&mut buf, "\n# Longest Time (and most messages) Between Different Users")?;
    let mut differences = Vec::new();

    let mut prev_text = dms.messages.iter().filter_map(Message::as_text_message).next().context("Expected a text message")?;
    let mut messages_between = 1_usize;

    for text in dms.messages.iter().filter_map(Message::as_text_message).skip(1) {
        if text.author != prev_text.author {
            differences.push((text.timestamp - prev_text.timestamp, prev_text.content.as_str(), text.content.as_str(), prev_text.id, text.id, prev_text.timestamp, text.timestamp, messages_between));
            prev_text = text;
            messages_between = 1;
        } else {
            messages_between += 1;
        }
    }
    
    differences.sort_by_key(|(a, _, _, _, _, _, _, _)| *a);
    for (idx, (diff, first_content, second_content, first_id, second_id, first_timestamp, second_timestamp, messages_between)) in differences.iter().rev().take(25).enumerate() {
        let difference = TimeQuantity::from(diff.num_milliseconds() as usize);
        writeln!(&mut buf, "{n}: diff = {difference}, messages_between = {messages_between}, first_timestamp = {first_timestamp}, second_timestamp = {second_timestamp}, first_id = {first_id}, second_id = {second_id} | first_content = {first_content:?} | second_content = {second_content:?}", n = idx + 1)?;
    }
    
    writeln!(&mut buf)?;

    differences.sort_by_key(|(_, _, _, _, _, _, _, a)| *a);
    for (idx, (diff, first_content, second_content, first_id, second_id, first_timestamp, second_timestamp, messages_between)) in differences.iter().rev().take(25).enumerate() {
        let difference = TimeQuantity::from(diff.num_milliseconds() as usize);
        writeln!(&mut buf, "{n}: messages_between = {messages_between}, diff = {difference}, first_timestamp = {first_timestamp}, second_timestamp = {second_timestamp}, first_id = {first_id}, second_id = {second_id} | first_content = {first_content:?} | second_content = {second_content:?}", n = idx + 1)?;
    }

    Ok(buf)
}

fn most_said_words(dms: &DirectMessages) -> Result<String> {
    let mut buf = String::new();

    writeln!(&mut buf, "\n# 100 Most Said Words")?;
    let mut map = FxHashMap::<String, usize>::default();

    for text in dms.messages.iter().filter_map(Message::as_text_message) {
        let content = text.content_alphanumeric_lowercase();
        for word in content.split_ascii_whitespace() {
            *map.entry(word.to_owned()).or_insert(0) += 1;
        }
    }

    writeln!(&mut buf, "anyway = {}", map["anyway"])?;
    writeln!(&mut buf, "fun = {}", map["fun"])?;

    let mut map = map.into_iter().collect::<Vec<_>>();
    map.sort_by_key(|(_, b)| usize::MAX - *b);
    for (idx, (word, count)) in map.into_iter().take(100).enumerate() {
        writeln!(&mut buf, "{n}: {word} ({count})", n = idx + 1, count = count.to_formatted_string(&Locale::en))?;
    }

    Ok(buf)
}

fn words_and_characters_written(dms: &DirectMessages) -> Result<String> {
    let mut buf = String::new();

    writeln!(&mut buf, "\n# Words and Characters Written (per person)")?;

    let mut map = FxHashMap::<&str, (usize, usize)>::default();
    for text in dms.messages.iter().filter_map(Message::as_text_message) {
        let written = text.content_alphanumeric_lowercase();
        let words = written.split_ascii_whitespace().count();
        let entry = map.entry(text.author.name.as_str()).or_insert((0, 0));
        // if entry.0 < milestone && entry.0 + words >= milestone && let Some(word) = written.split_ascii_whitespace().skip(entry.0 + words - milestone).next() {
        //     writeln!(&mut buf, "{author}'s {milestone_nth} word was '{word}' (https://discord.com/channels/@me/{author_id}/{msg_id}) @ {datetime}", author = text.author.name.as_str(), milestone_nth = nth(milestone), author_id = text.author.id, msg_id = text.id, datetime = text.timestamp);
        // }
        entry.0 += words;
        entry.1 += text.content.len();
    }

    for (author, (words, characters)) in map.into_iter() {
        writeln!(&mut buf, "{author} has written {words} words and {characters} characters", words = words.to_formatted_string(&Locale::en), characters = characters.to_formatted_string(&Locale::en))?;
    }

    Ok(buf)
}

fn most_characters_said_in_a_day(dms: &DirectMessages) -> Result<String> {
    #[derive(Default)]
    struct Measurement {
        messages: usize,
        words: usize,
        characters: usize,
        attachments: usize,
    }

    let mut buf = String::new();

    writeln!(&mut buf, "\n# Most Messages, Words, Characters, and Attachments Said In Day (sorted by messages)")?;

    let mut map = FxHashMap::<NaiveDate, Measurement>::default();
    for text in dms.messages.iter().filter_map(Message::as_text_message) {
        let chars = text.content.len();
        let written = text.content.to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric() || c.is_ascii_whitespace()).collect::<String>();
        let date = text.timestamp.date();
        let entry = map.entry(date).or_insert(Measurement::default());
        entry.messages += 1;
        entry.words += written.split_ascii_whitespace().count();
        entry.characters += chars;
        entry.attachments += text.attachments.len();
    }

    let mut map = map.into_iter().collect::<Vec<_>>();
    map.sort_by_key(|(_, b)| usize::MAX - b.messages);
    for (idx, (date, measurement)) in map.into_iter().take(25).enumerate() {
        writeln!(&mut buf, "{n}: {date}: messages = {messages}, words = {words}, characters = {characters}, attachments = {attachments}", n = idx + 1, messages = measurement.messages.to_formatted_string(&Locale::en), words = measurement.words.to_formatted_string(&Locale::en), characters = measurement.characters.to_formatted_string(&Locale::en), attachments = measurement.attachments.to_formatted_string(&Locale::en))?;
    }

    Ok(buf)
}

fn call_start_time_of_day_graph(dms: &DirectMessages) -> Result<String> {
    let mut buf = String::new();

    writeln!(&mut buf, "\n# Call Start Time of Day Graph (min = 15s, 15m groupings)")?;

    let mut graph = Graph::new(dms.channel.authors.clone(), 5 * 4 + 2, |idx| format!("{hours:02}h{minutes:02}m", hours = idx / 4, minutes = (idx % 4) * 15), dataset_sum, 50);

    for call in dms.messages.iter().filter_map(Message::as_call).filter(|call | call.duration() >= TimeDelta::seconds(15)) {
        let datetime = call.start_timestamp;
        let time = datetime.time();
        let index = (time.hour() * 4 + time.minute() / 15) as usize;
        graph.add(&call.author.name, index, 1);
    }

    writeln!(&mut buf, "{graph}")?;

    Ok(buf)
}

fn text_time_of_day_graph(dms: &DirectMessages) -> Result<String> {
    let mut buf = String::new();

    writeln!(&mut buf, "\n# Text Time of Day Graph (10m groupings)")?;

    let mut graph = Graph::new(dms.channel.authors.clone(), 5 * 6 + 3, |idx| format!("{hours:02}h{minutes:02}m", hours = idx / 6, minutes = (idx % 6) * 10), dataset_sum, 50);

    for text in dms.messages.iter().filter_map(Message::as_text_message) {
        let datetime = text.timestamp;
        let time = datetime.time();
        let index = (time.hour() * 6 + time.minute() / 10) as usize;
        graph.add(&text.author.name, index, 1);
    }

    writeln!(&mut buf, "{graph}")?;

    Ok(buf)
}

fn call_duration_by_month_graph(dms: &DirectMessages) -> Result<String> {
    let mut buf = String::new();

    writeln!(&mut buf, "\n# Call Duration by Month Graph (min = 15s)")?;

    let mut graph = Graph::new(vec![dms.channel.name.as_str()], 0, |idx| format!("{month}", month = NaiveDate::from_ymd_opt(1, (idx + 1) as u32, 1).expect("Valid date").format("%h")), dataset_average, 50);

    for call in dms.messages.iter().filter_map(Message::as_call).filter(|call | call.duration() >= TimeDelta::seconds(15)) {
        let datetime = call.start_timestamp;
        let date = datetime.date();
        let index = date.month0() as usize;
        graph.add(&dms.channel.name, index, TimeQuantity::from(call.duration()));
    }

    writeln!(&mut buf, "{graph}")?;

    Ok(buf)
}

fn call_duration_by_day_of_week_graph(dms: &DirectMessages) -> Result<String> {
    let mut buf = String::new();

    writeln!(&mut buf, "\n# Call Duration by Day of Week Graph (min = 15s)")?;

    let mut graph = Graph::new(vec![dms.channel.name.as_str()], 0, |idx| Weekday::from_usize(idx).unwrap().to_string(), dataset_average, 50);

    for call in dms.messages.iter().filter_map(Message::as_call).filter(|call | call.duration() >= TimeDelta::seconds(15)) {
        let datetime = call.start_timestamp;
        let index = datetime.date().weekday() as usize;
        graph.add(&dms.channel.name, index, TimeQuantity::from(call.duration()));
    }

    writeln!(&mut buf, "{graph}")?;

    Ok(buf)
}

fn call_graph(dms: &DirectMessages) -> Result<String> {
    let mut buf = String::new();

    writeln!(&mut buf, "\n# Call Graph (10m groupings, min = 15s)")?;

    let mut graph = Graph::new(dms.channel.authors.clone(), 5 * 6 + 3, |idx| format!("{hours:02}h{minutes:02}m", hours = idx / 6, minutes = (idx % 6) * 10), dataset_sum, 50);

    for call in dms.messages.iter().filter_map(Message::as_call).filter(|call | call.duration() >= TimeDelta::seconds(15)) {
        let start_time = call.start_timestamp;
        let start_time_start = start_time.with_minute(start_time.minute() / 10 * 10).unwrap().with_second(0).unwrap().with_nanosecond(0).unwrap();
        let mut index = (start_time.hour() * 6 + start_time.minute() / 10) as usize;
        let end_time = call.end_timestamp;
        let end_time_start = end_time.with_minute(end_time.minute() / 10 * 10).unwrap().with_second(0).unwrap().with_nanosecond(0).unwrap();
        let head_duration = (start_time - start_time_start).num_milliseconds() as usize;
        graph.add(&call.author.name, index, TimeQuantity::from(head_duration));
        index += 1;
        if start_time_start != end_time_start {
            let mut remaining_millis = (call.duration().num_milliseconds() as usize).saturating_sub(head_duration);
            while remaining_millis > 0 {
                graph.add(&call.author.name, index % (24 * 6), TimeQuantity::from(remaining_millis.min(10 * 60 * 1000)));
                remaining_millis = remaining_millis.saturating_sub(10 * 60 * 1000);
                index += 1;
            }
        }
    }

    writeln!(&mut buf, "{graph}")?;

    Ok(buf)
}

fn call_png(dms: &DirectMessages) -> Result<String> {
    const RED_CHANNEL: [u8; 3] = [0x98, 0xE5, 0x5E];
    const GREEN_CHANNEL: [u8; 3] = [0xC3, 0xC0, 0xAC];
    const BLUE_CHANNEL: [u8; 3] = [0x79, 0x7B, 0xEC];

    let mut buf = String::new();

    writeln!(&mut buf, "\n# Generating Call Graph Image (1m groupings)...")?;

    writeln!(&mut buf, "Collecting Raw Data...")?;

    const NUM_QUANTITIES: usize = 24 * 60 * 4;
    const QUANTITY_PER: usize = 1000 * 60 * 60 * 24 / NUM_QUANTITIES;
    let mut quantities: [Vec<usize>; NUM_QUANTITIES] = std::array::from_fn(|_| vec![0_usize; dms.channel.authors.len()]);

    for call in dms.messages.iter().filter_map(Message::as_call).filter(|call | call.duration() >= TimeDelta::seconds(15)) {
        let author_idx = dms.channel.authors.iter().position(|author| *author == call.author.name).unwrap();
        let start_time = call.start_timestamp;
        let start_time_start = start_time.with_second(0).unwrap().with_nanosecond(0).unwrap();
        let mut index = (((start_time.hour() * 60 + start_time.minute()) * 60 + start_time.second()) * 1000) as usize / QUANTITY_PER;
        let end_time = call.end_timestamp;
        let end_time_start = end_time.with_second(0).unwrap().with_nanosecond(0).unwrap();
        let head_duration = (start_time - start_time_start).num_milliseconds() as usize;
        quantities[index][author_idx] += head_duration;
        index = (index + 1) % NUM_QUANTITIES;
        if start_time_start != end_time_start {
            let mut remaining_millis = (call.duration().num_milliseconds() as usize).saturating_sub(head_duration);
            while remaining_millis > 0 {
                quantities[index][author_idx] += remaining_millis.min(QUANTITY_PER);
                remaining_millis = remaining_millis.saturating_sub(QUANTITY_PER);
                index = (index + 1) % NUM_QUANTITIES;
            }
        }
    }

    let (width, height) = (quantities.len(), (quantities.len() as f64 / (std::f64::consts::TAU)).ceil() as usize);
    let max_ms = quantities.iter().map(|x| x.iter().copied().sum::<usize>() + 1).max().unwrap_or(0);
    let ms_per_px = max_ms.div_ceil(height);
    writeln!(&mut buf, "Generating Base Image...")?;
    let mut image = image::RgbaImage::from_pixel(width as u32, height as u32, Rgba([0x31, 0x33, 0x38, 0xFF]));
    for x in 0..width {
        print!("Generating bars ({x} / {width}) ({pct:.1}%)...\r", pct = 100.0 * x as f64 / width as f64);
        std::io::Write::flush(&mut stdout())?;
        let quantities_index = (x + 11 * NUM_QUANTITIES / 48) % width;
        let section = &*quantities[quantities_index];
        let heights = (0..section.len()).map(|idx| height - 1 - section.iter().copied().take(idx).map(|x| x / ms_per_px).sum::<usize>()).collect::<Vec<_>>();
        for (idx, (mut remaining_quantity, mut y)) in section.iter().copied().zip(heights.into_iter()).enumerate().rev() {
            while remaining_quantity > 0 {
                image.get_pixel_mut(x as u32, y as u32).blend(&Rgba([RED_CHANNEL[idx % RED_CHANNEL.len()], GREEN_CHANNEL[idx % GREEN_CHANNEL.len()], BLUE_CHANNEL[idx % BLUE_CHANNEL.len()], (remaining_quantity.min(ms_per_px) * 0xFF / ms_per_px) as u8]));
                remaining_quantity = remaining_quantity.saturating_sub(ms_per_px);
                y = y.saturating_sub(1);
            }
        }
    }

    writeln!(&mut buf, "Generating bars ({width} / {width}) (100.0%)...")?;
    writeln!(&mut buf, "Writing file...")?;

    let mut file = File::create(format!("Call Graph - {channel_name} - {id}.png", channel_name = dms.channel.name, id = dms.channel.id))?;
    image.write_to(&mut file, ImageFormat::Png)?;

    writeln!(&mut buf, "# Generated Call Graph Image")?;

    Ok(buf)
}

fn capitalization_rates(dms: &DirectMessages) -> Result<String> {
    let mut buf = String::new();

    writeln!(&mut buf, "\n# Capitalization Rates")?;

    let first_year = dms.messages.iter().filter_map(Message::as_text_message).map(|text| text.timestamp).min().context("Expected at least one message sent")?.year();
    let last_year = dms.messages.iter().filter_map(Message::as_text_message).map(|text| text.timestamp).max().context("Expected at least one message sent")?.year();

    for year in first_year..=last_year {
        let mut quantities = vec![(0_usize, 0_usize); dms.channel.authors.len()];

        for text in dms.messages.iter().filter_map(Message::as_text_message).filter(|text| text.timestamp.year() == year && text.content.as_str().chars().next().is_some_and(char::is_alphabetic)) {
            let author_idx = dms.channel.authors.iter().position(|author| *author == text.author.name).unwrap();
            let (capitalized, uncapitalized) = &mut quantities[author_idx];
            if text.content.as_str().chars().next().is_some_and(char::is_uppercase) {
                *capitalized += 1;
            } else {
                *uncapitalized += 1;
            }
        }

        writeln!(&mut buf, "\n## {year}")?;

        for (author_idx, (capitalized, uncapitalized)) in quantities.into_iter().enumerate() {
            let total = capitalized + uncapitalized;
            let author_name = dms.channel.authors[author_idx];
            writeln!(&mut buf, "{author_name}: {capitalized} / {total} ({pct:.2}%)", pct = 100.0 * capitalized as f64 / total as f64)?;
        }
    }

    Ok(buf)
}

fn edit_rates(dms: &DirectMessages) -> Result<String> {
    let mut buf = String::new();

    writeln!(&mut buf, "\n# Edit Rates")?;

    let first_year = dms.messages.iter().filter_map(Message::as_text_message).map(|text| text.timestamp).min().context("Expected at least one message sent")?.year();
    let last_year = dms.messages.iter().filter_map(Message::as_text_message).map(|text| text.timestamp).max().context("Expected at least one message sent")?.year();

    for year in first_year..=last_year {
        let mut quantities = vec![(0_usize, 0_usize); dms.channel.authors.len()];

        for text in dms.messages.iter().filter_map(Message::as_text_message).filter(|text| text.timestamp.year() == year) {
            let author_idx = dms.channel.authors.iter().position(|author| *author == text.author.name).unwrap();
            let (edited, unedited) = &mut quantities[author_idx];
            if text.edited_timestamp.is_some() {
                *edited += 1;
            } else {
                *unedited += 1;
            }
        }

        writeln!(&mut buf, "\n## {year}")?;

        for (author_idx, (edited, unedited)) in quantities.into_iter().enumerate() {
            let total = edited + unedited;
            let author_name = dms.channel.authors[author_idx];
            writeln!(&mut buf, "{author_name}: {edited} / {total} ({pct:.2}%)", pct = 100.0 * edited as f64 / total as f64)?;
        }
    }

    Ok(buf)
}
