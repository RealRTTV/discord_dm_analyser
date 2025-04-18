use std::fmt::{Debug, Display, Formatter};
use std::iter;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign};
use chrono::TimeDelta;
use itertools::Itertools;
use crate::{generate_progress_bar, standard_deviation};

#[derive(Copy, Clone, Default)]
pub struct TimeQuantity {
    days: usize,
    hours: usize,
    minutes: usize,
    seconds: usize,
    milliseconds: usize,
}

impl TimeQuantity {
    pub const ZERO: Self = Self::new(0, 0, 0, 0, 0);

    pub const fn new(days: usize, hours: usize, minutes: usize, seconds: usize, milliseconds: usize) -> Self {
        Self {
            days,
            hours,
            minutes,
            seconds,
            milliseconds,
        }
    }
}

impl From<usize> for TimeQuantity {
    fn from(mut ms: usize) -> Self {
        let days = ms / (1000 * 60 * 60 * 24);
        ms -= days * (1000 * 60 * 60 * 24);
        let hours = ms / (1000 * 60 * 60);
        ms -= hours * (1000 * 60 * 60);
        let minutes = ms / (1000 * 60);
        ms -= minutes * (1000 * 60);
        let seconds = ms / 1000;
        ms -= seconds * 1000;
        let milliseconds = ms;
        Self {
            days,
            hours,
            minutes,
            seconds,
            milliseconds,
        }
    }
}

impl From<TimeDelta> for TimeQuantity {
    fn from(value: TimeDelta) -> Self {
        Self::from(value.num_milliseconds().max(0) as usize)
    }
}

impl From<TimeQuantity> for usize {
    fn from(time: TimeQuantity) -> Self {
        time.days * (1000 * 60 * 60 * 24)
            + time.hours * (1000 * 60 * 60)
            + time.minutes * (1000 * 60)
            + time.seconds * (1000)
            + time.milliseconds
    }
}

impl Add for TimeQuantity {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::from(Into::<usize>::into(self) + Into::<usize>::into(rhs))
    }
}

impl AddAssign for TimeQuantity {
    fn add_assign(&mut self, rhs: Self) {
        *self = self.clone() + rhs;
    }
}

impl Div<usize> for TimeQuantity {
    type Output = Self;

    fn div(self, rhs: usize) -> Self::Output {
        Self::from(Into::<usize>::into(self).checked_div(rhs).unwrap_or(0))
    }
}

impl DivAssign<usize> for TimeQuantity {
    fn div_assign(&mut self, rhs: usize) {
        *self = self.clone() / rhs;
    }
}

impl Mul<usize> for TimeQuantity {
    type Output = Self;

    fn mul(self, rhs: usize) -> Self::Output {
        Self::from(Into::<usize>::into(self) * rhs)
    }
}

impl MulAssign<usize> for TimeQuantity {
    fn mul_assign(&mut self, rhs: usize) {
        *self = self.clone() * rhs;
    }
}

impl Sum for TimeQuantity {
    fn sum<I: Iterator<Item=Self>>(iter: I) -> Self {
        iter.fold(TimeQuantity::ZERO, |acc, x| acc + x)
    }
}

impl Display for TimeQuantity {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Self { days, hours, minutes, seconds, milliseconds } = *self;
        if days > 0 {
            write!(f, "{days}d{hours:02}h{minutes:02}m{seconds:02}s{milliseconds:03}ms")
        } else if hours > 0 {
            write!(f, "{hours:02}h{minutes:02}m{seconds:02}s{milliseconds:03}ms")
        } else if minutes > 0 {
            write!(f, "{minutes:02}m{seconds:02}s{milliseconds:03}ms")
        } else if seconds > 0 {
            write!(f, "{seconds:02}s{milliseconds:03}ms")
        } else {
            write!(f, "{milliseconds:03}ms")
        }
    }
}

impl Debug for TimeQuantity {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Self { days, hours, minutes, seconds, milliseconds } = *self;
        write!(f, "{days}d{hours:02}h{minutes:02}m{seconds:02}s{milliseconds:03}ms")
    }
}

pub struct Graph<'a, T: From<usize>, S: Fn(&[T]) -> usize, F: Fn(usize) -> String> {
    labels: Vec<String>,
    authors: Box<[&'a str]>,
    data: Vec<Box<[Vec<T>]>>,
    start_idx: usize,
    width: usize,
    sum: S,
    label_fn: F,
}

impl<'a, T: From<usize>, S: Fn(&[T]) -> usize, F: Fn(usize) -> String> Graph<'a, T, S, F> {
    pub fn new(authors: impl Into<Box<[&'a str]>>, start_idx: usize, label_fn: F, sum: S, width: usize) -> Self {
        let authors = authors.into();
        Self {
            labels: Vec::new(),
            data: Vec::new(),
            start_idx,
            authors,
            width,
            sum,
            label_fn,
        }
    }

    pub fn add(&mut self, author: &str, idx: usize, quantity: T) -> bool {
        let Some(author_index) = self.authors.iter().position(|x| *x == author) else { return false };
        if self.data.len() <= idx {
            self.data.extend(iter::from_fn(|| Some(Box::<[Vec<T>]>::from_iter(iter::from_fn(|| Some(Vec::new())).take(self.authors.len())))).take(idx + 1 - self.data.len()));
            let mut label_idx = self.labels.len();
            self.labels.extend(iter::from_fn(|| {
                let label = (self.label_fn)(label_idx);
                label_idx += 1;
                Some(label)
            }).take(idx + 1 - self.labels.len()));
        }
        let Some(line) = self.data.get_mut(idx) else { return false };
        line[author_index].push(quantity);
        true
    }
}

impl<T: From<usize>, S: Fn(&[T]) -> usize, F: Fn(usize) -> String> Display for Graph<'_, T, S, F> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        const FULL_CHAR: char = '#';
        const EMPTY_CHAR: char = '-';

        let (min, max) = self.data.iter().map(|line| line.iter().map(|u| (&self.sum)(&u)).sum::<usize>()).minmax().into_option().unwrap_or((0, 0));
        let sum = self.data.iter().map(|line| line.iter().map(|u| (&self.sum)(&u)).sum::<usize>()).sum::<usize>();
        let mean = sum as f64 / self.data.len() as f64;
        let sd = standard_deviation(sum, self.data.iter().map(|line| line.iter().map(|u| (&self.sum)(&u)).sum::<usize>()), self.data.len());

        writeln!(f, "Graph Data: sum = {sum}, mean = {mean}, sd = {sd}, width = {width}, min = {min}, max = {max}", width = self.width)?;
        writeln!(f, "Legend:")?;
        for (idx, author) in self.authors.iter().enumerate() {
            writeln!(f, "\x1B[{color}m{author}\x1B[0m", color = 92 + idx % 5)?
        }
        for (idx, quantities) in (self.start_idx..self.data.len()).chain(0..self.start_idx).map(|idx| (idx, &self.data[idx])) {
            writeln!(f, "{label} | {bar}", label = self.labels[idx], bar = generate_progress_bar(self.width, FULL_CHAR, EMPTY_CHAR, max, &quantities, |vec| (&self.sum)(&vec)))?;
        }

        Ok(())
    }
}

pub fn dataset_sum<T: Sum + Into<usize> + Clone>(data: &[T]) -> usize {
    data.iter().cloned().sum::<T>().into()
}

pub fn dataset_average<T: Sum + Div<usize> + Into<usize> + Clone>(data: &[T]) -> usize where <T as Div<usize>>::Output: Into<usize> {
    (data.iter().cloned().sum::<T>() / data.len()).into()
}
