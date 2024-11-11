#![allow(incomplete_features)]
#![allow(dead_code)]

#![feature(build_hasher_default_const_new)]
#![feature(generic_const_exprs)]
#![feature(const_collections_with_hasher)]

pub mod data;
pub mod serde_structs;

use std::fmt::Write;
use std::fs::File;
use std::path::Path;
use std::time::Instant;
use chrono::{DateTime, Datelike, Local, NaiveDate, Timelike, Utc, Weekday};
use fxhash::FxHashMap;
use itertools::Itertools;
use num_format::{Locale, ToFormattedString};
use anyhow::{Context, Result};
use image::{ImageFormat, Pixel, Rgba};
use num_traits::FromPrimitive;
use crate::data::{dataset_average, dataset_sum, Graph, TimeQuantity};
use crate::serde_structs::{Call, DirectMessages, Message, UninitDirectMessages};

fn main() -> Result<()> {
    let Some(path) = std::env::args().nth(1) else {
        println!("No path specified; usage: discord_dm_analyser <file>");
        std::process::exit(0);
    };

    let result = parse_dms(&path);

    result.context("Failed to evalulate DM information")?;

    Ok(())
}

fn parse_dms<P: AsRef<Path>>(path: P) -> Result<()> {
    println!("Parsing DMs...");
    let start = Instant::now();
    let dms: DirectMessages = serde_json::from_slice::<UninitDirectMessages>(&std::fs::read(path)?)?.try_into()?;
    println!("Parsed DMs in {}", TimeQuantity::from(start.elapsed().as_millis() as usize));

    top_call_lengths(&dms)?;
    total_call_lengths(&dms)?;
    // longest_time_between_messages(&dms)?;
    // most_said_words(&dms)?;
    // words_and_characters_written(&dms)?;
    // most_characters_said_in_a_day(&dms)?;
    // call_start_time_of_day_graph(&dms)?;
    // text_time_of_day_graph(&dms)?;
    // call_duration_by_week_graph(&dms)?;
    // call_duration_by_day_of_week_graph(&dms)?;
    // call_graph(&dms)?;
    call_png(&dms)?;

    Ok(())
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
        let _ = write!(&mut buf, "\x1B[0m");
    }
    let _ = buf.write_str(&empty_char.to_string().repeat(width.saturating_sub(current_quantity)));
    let _ = buf.write_char(']');
    buf
}

fn top_call_lengths(dms: &DirectMessages) -> Result<()> {
    println!("\n# Top 25 Call Lengths");
    let mut lengths = dms.messages
        .iter()
        .filter_map(Message::as_call)
        .map(Call::duration)
        .collect::<Vec<_>>();

    lengths.sort();

    println!("total calls: {}", lengths.len());

    for (idx, duration) in lengths.into_iter().rev().take(25).enumerate() {
        let len = TimeQuantity::from(duration as usize);
        println!("{n}: length = {len:?}", n = idx + 1);
    }

    Ok(())
}

fn total_call_lengths(dms: &DirectMessages) -> Result<()> {
    println!("\n# Total Call Lengths");
    let len = TimeQuantity::from(dms.messages
        .iter()
        .filter_map(Message::as_call)
        .map(Call::duration)
        .sum::<u64>() as usize);

    println!("total length = {len}");

    Ok(())
}

fn longest_time_between_messages(dms: &DirectMessages) -> Result<()> {
    println!("\n# Longest Time Between Messages");
    let mut differences = Vec::new();

    for (a, b) in dms.messages.iter().filter_map(Message::as_text_message).tuple_windows() {
        differences.push((b.timestamp.saturating_sub(a.timestamp), a.content.as_str(), a.author.name.as_str(), a.id, b.id, a.timestamp, b.timestamp));
    }

    differences.sort_by_key(|(a, _, _, _, _, _, _)| *a);

    for (idx, (diff, content, author, first_id, second_id, first_timestamp, second_timestamp)) in differences.into_iter().rev().take(25).enumerate() {
        let difference = TimeQuantity::from(diff as usize);
        let first_timestamp = DateTime::<Utc>::from_timestamp_millis(first_timestamp as i64).context("Could not parse timestamp")?.with_timezone(&Local).naive_local();
        let second_timestamp = DateTime::<Utc>::from_timestamp_millis(second_timestamp as i64).context("Could not parse timestamp")?.with_timezone(&Local).naive_local();
        println!("{n}: diff = {difference}, first_timestamp = {first_timestamp}, second_timestamp = {second_timestamp}, first_id = {first_id}, second_id = {second_id} | content = {content:?}, author = {author}", n = idx + 1);
    }

    Ok(())
}

fn most_said_words(dms: &DirectMessages) -> Result<()> {
    println!("\n# 100 Most Said Words");
    let mut map = FxHashMap::<String, usize>::default();

    for text in dms.messages.iter().filter_map(Message::as_text_message) {
        let content = text.content_alphanumeric_lowercase();
        for word in content.split_ascii_whitespace() {
            *map.entry(word.to_owned()).or_insert(0) += 1;
        }
    }

    println!("anyway = {}", map["anyway"]);
    println!("fun = {}", map["fun"]);

    let mut map = map.into_iter().collect::<Vec<_>>();
    map.sort_by_key(|(_, b)| usize::MAX - *b);
    for (idx, (word, count)) in map.into_iter().take(100).enumerate() {
        println!("{n}: {word} ({count})", n = idx + 1, count = count.to_formatted_string(&Locale::en));
    }

    Ok(())
}

fn words_and_characters_written(dms: &DirectMessages) -> Result<()> {
    println!("\n# Words and Characters Written (per person)");

    let mut map = FxHashMap::<&str, (usize, usize)>::default();
    for text in dms.messages.iter().filter_map(Message::as_text_message) {
        let written = text.content_alphanumeric_lowercase();
        let entry = map.entry(text.author.name.as_str()).or_insert((0, 0));
        entry.0 += written.split_ascii_whitespace().count();
        entry.1 += text.content.len();
    }

    for (author, (words, characters)) in map.into_iter() {
        println!("{author} has written {words} words and {characters} characters", words = words.to_formatted_string(&Locale::en), characters = characters.to_formatted_string(&Locale::en));
    }

    Ok(())
}

fn most_characters_said_in_a_day(dms: &DirectMessages) -> Result<()> {
    #[derive(Default)]
    struct Measurement {
        messages: usize,
        words: usize,
        characters: usize,
        attachments: usize,
    }

    println!("\n# Most Messages, Words, Characters, and Attachments Said In Day (sorted by messages)");

    let mut map = FxHashMap::<NaiveDate, Measurement>::default();
    for text in dms.messages.iter().filter_map(Message::as_text_message) {
        let chars = text.content.len();
        let written = text.content.to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric() || c.is_ascii_whitespace()).collect::<String>();
        let date = DateTime::<Utc>::from_timestamp_millis(text.timestamp as i64).context("Could not parse timestamp")?.with_timezone(&Local).naive_local().date();
        let entry = map.entry(date).or_insert(Measurement::default());
        entry.messages += 1;
        entry.words += written.split_ascii_whitespace().count();
        entry.characters += chars;
        entry.attachments += text.attachments.len();
    }

    let mut map = map.into_iter().collect::<Vec<_>>();
    map.sort_by_key(|(_, b)| usize::MAX - b.messages);
    for (idx, (date, measurement)) in map.into_iter().take(25).enumerate() {
        println!("{n}: {date}: messages = {messages}, words = {words}, characters = {characters}, attachments = {attachments}", n = idx + 1, messages = measurement.messages.to_formatted_string(&Locale::en), words = measurement.words.to_formatted_string(&Locale::en), characters = measurement.characters.to_formatted_string(&Locale::en), attachments = measurement.attachments.to_formatted_string(&Locale::en));
    }

    Ok(())
}

fn call_start_time_of_day_graph(dms: &DirectMessages) -> Result<()> {
    println!("\n# Call Start Time of Day Graph (min = 15s, 15m groupings)");

    let mut graph = Graph::<{ 24 * 4 }, usize, _>::new(dms.channel.authors.clone(), 5 * 4 + 2, |idx| format!("{hours:02}h{minutes:02}m", hours = idx / 4, minutes = (idx % 4) * 15), dataset_sum, 50);

    for call in dms.messages.iter().filter_map(Message::as_call).filter(|call | call.duration() >= 15_000) {
        let datetime = DateTime::<Utc>::from_timestamp_millis(call.start_timestamp as i64).context("Could not parse timestamp")?.with_timezone(&Local).naive_local();
        let time = datetime.time();
        let index = (time.hour() * 4 + time.minute() / 15) as usize;
        graph.add(&call.author.name, index, 1);
    }

    println!("{graph}");

    Ok(())
}

fn text_time_of_day_graph(dms: &DirectMessages) -> Result<()> {
    println!("\n# Text Time of Day Graph (10m groupings)");

    let mut graph = Graph::<'_, { 24 * 6 }, usize, _>::new(dms.channel.authors.clone(), 5 * 6 + 3, |idx| format!("{hours:02}h{minutes:02}m", hours = idx / 6, minutes = (idx % 6) * 10), dataset_sum, 50);

    for text in dms.messages.iter().filter_map(Message::as_text_message) {
        let datetime = DateTime::<Utc>::from_timestamp_millis(text.timestamp as i64).context("Could not parse timestamp")?.with_timezone(&Local).naive_local();
        let time = datetime.time();
        let index = (time.hour() * 6 + time.minute() / 10) as usize;
        graph.add(&text.author.name, index, 1);
    }

    println!("{graph}");

    Ok(())
}

fn call_duration_by_week_graph(dms: &DirectMessages) -> Result<()> {
    println!("\n# Call Duration by Month Graph (min = 15s)");

    let mut graph = Graph::<'_, 12, TimeQuantity, _>::new(vec![dms.channel.name.as_str()], 0, |idx| format!("{month}", month = NaiveDate::from_ymd_opt(1, (idx + 1) as u32, 1).expect("Valid date").format("%h")), dataset_average, 50);

    for call in dms.messages.iter().filter_map(Message::as_call).filter(|call| call.duration() >= 15_000) {
        let datetime = DateTime::<Utc>::from_timestamp_millis(call.start_timestamp as i64).context("Could not parse timestamp")?.with_timezone(&Local).naive_local();
        let date = datetime.date();
        let index = date.month0() as usize;
        graph.add(&dms.channel.name, index, TimeQuantity::from(call.duration() as usize));
    }

    println!("{graph}");

    Ok(())
}

fn call_duration_by_day_of_week_graph(dms: &DirectMessages) -> Result<()> {
    println!("\n# Call Duration by Day of Week Graph (min = 15s)");

    let mut graph = Graph::<'_, 7, TimeQuantity, _>::new(vec![dms.channel.name.as_str()], 0, |idx| Weekday::from_usize(idx).unwrap().to_string(), dataset_average, 50);

    for call in dms.messages.iter().filter_map(Message::as_call).filter(|call| call.duration() >= 15_000) {
        let datetime = DateTime::<Utc>::from_timestamp_millis(call.start_timestamp as i64).context("Could not parse timestamp")?.with_timezone(&Local).naive_local();

        let index = datetime.date().weekday() as usize;
        graph.add(&dms.channel.name, index, TimeQuantity::from(call.duration() as usize));
    }

    println!("{graph}");

    Ok(())
}

fn call_graph(dms: &DirectMessages) -> Result<()> {
    println!("\n# Call Graph (10m groupings, min = 15s)");

    let mut graph = Graph::<'_, { 24 * 6 }, TimeQuantity, _>::new(dms.channel.authors.clone(), 5 * 6 + 3, |idx| format!("{hours:02}h{minutes:02}m", hours = idx / 6, minutes = (idx % 6) * 10), dataset_sum, 50);

    for call in dms.messages.iter().filter_map(Message::as_call).filter(|call| call.duration() >= 15_000) {
        let start_time = DateTime::<Utc>::from_timestamp_millis(call.start_timestamp as i64).context("Could not parse timestamp")?.with_timezone(&Local).naive_local();
        let start_time_start = start_time.with_minute(start_time.minute() / 10 * 10).unwrap().with_second(0).unwrap().with_nanosecond(0).unwrap();
        let mut index = (start_time.hour() * 6 + start_time.minute() / 10) as usize;
        let end_time = DateTime::<Utc>::from_timestamp_millis(call.end_timestamp as i64).context("Could not parse timestamp")?.with_timezone(&Local).naive_local();
        let end_time_start = end_time.with_minute(end_time.minute() / 10 * 10).unwrap().with_second(0).unwrap().with_nanosecond(0).unwrap();
        let head_duration = (start_time - start_time_start).num_milliseconds() as usize;
        graph.add(&call.author.name, index, TimeQuantity::from(head_duration));
        index += 1;
        if start_time_start != end_time_start {
            let mut remaining_millis = (call.duration() as usize).saturating_sub(head_duration);
            while remaining_millis > 0 {
                graph.add(&call.author.name, index % (24 * 6), TimeQuantity::from(remaining_millis.min(10 * 60 * 1000)));
                remaining_millis = remaining_millis.saturating_sub(10 * 60 * 1000);
                index += 1;
            }
        }
    }

    println!("{graph}");

    Ok(())
}

fn call_png(dms: &DirectMessages) -> Result<()> {
    const RED_CHANNEL: [u8; 3] = [0x98, 0xE5, 0x5E];
    const GREEN_CHANNEL: [u8; 3] = [0xC3, 0xC0, 0xAC];
    const BLUE_CHANNEL: [u8; 3] = [0x79, 0x7B, 0xEC];

    println!("\n# Generating Call Graph Image (1m groupings)...");

    println!("Collecting Raw Data...");

    const NUM_QUANTITIES: usize = 24 * 60 * 4;
    const QUANTITY_PER: usize = 1000 * 60 * 60 * 24 / NUM_QUANTITIES;
    let mut quantities: [Vec<usize>; NUM_QUANTITIES] = std::array::from_fn(|_| vec![0_usize; dms.channel.authors.len()]);

    for call in dms.messages.iter().filter_map(Message::as_call).filter(|call| call.duration() >= 15_000) {
        let author_idx = dms.channel.authors.iter().position(|author| *author == call.author.name).unwrap();
        let start_time = DateTime::<Utc>::from_timestamp_millis(call.start_timestamp as i64).context("Could not parse timestamp")?.with_timezone(&Local).naive_local();
        let start_time_start = start_time.with_second(0).unwrap().with_nanosecond(0).unwrap();
        let mut index = (((start_time.hour() * 60 + start_time.minute()) * 60 + start_time.second()) * 1000) as usize / QUANTITY_PER;
        let end_time = DateTime::<Utc>::from_timestamp_millis(call.end_timestamp as i64).context("Could not parse timestamp")?.with_timezone(&Local).naive_local();
        let end_time_start = end_time.with_second(0).unwrap().with_nanosecond(0).unwrap();
        let head_duration = (start_time - start_time_start).num_milliseconds() as usize;
        quantities[index][author_idx] += head_duration;
        index = (index + 1) % NUM_QUANTITIES;
        if start_time_start != end_time_start {
            let mut remaining_millis = (call.duration() as usize).saturating_sub(head_duration);
            while remaining_millis > 0 {
                quantities[index][author_idx] += remaining_millis.min(QUANTITY_PER);
                remaining_millis = remaining_millis.saturating_sub(QUANTITY_PER);
                index = (index + 1) % NUM_QUANTITIES;
            }
        }
    }

    let (width, height) = (quantities.len(), (quantities.len() as f64 / (64.0 / 9.0)).ceil() as usize);
    let max_ms = quantities.iter().map(|x| x.iter().copied().sum::<usize>() + 1).max().unwrap_or(0);
    let ms_per_px = max_ms.div_ceil(height);
    println!("Generating Base Image...");
    let mut image = image::RgbaImage::from_pixel(width as u32, height as u32, Rgba([0x31, 0x33, 0x38, 0xFF]));
    for x in 0..width {
        print!("Generating bars ({x} / {width}) ({pct:.1}%)...\r", pct = 100.0 * x as f64 / width as f64);
        std::io::Write::flush(&mut std::io::stdout())?;
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

    println!("Generating bars ({width} / {width}) (100.0%)...");
    println!("Writing file...");

    let mut file = File::create(format!("Call Graph - {channel_name} - {id}.png", channel_name = dms.channel.name, id = dms.channel.id))?;
    image.write_to(&mut file, ImageFormat::Png)?;

    println!("# Generated Call Graph Image");

    Ok(())
}
