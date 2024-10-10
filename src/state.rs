use std::{fmt::{self, Debug, Display}, str::FromStr, time::{Duration, Instant}};
use log::warn;

#[derive(Clone, Copy, PartialEq)]
pub struct Point3<T> {
    pub x: T,
    pub y: T,
    pub z: T,
}

impl <T> Point3<T> {
    pub fn new(x: T, y: T, z: T) -> Self {
        Self{x, y, z}
    }
    pub fn apply(self, apply_f: impl Fn(T) -> T) -> Self {
        Self{x: apply_f(self.x), y: apply_f(self.y), z: apply_f(self.z)}
    }
    pub fn apply_other(self, other: Self, apply_f: impl Fn(T, T) -> T) -> Self {
        Self{x: apply_f(self.x, other.x), y: apply_f(self.y, other.y), z: apply_f(self.z, other.z)}
    }
}

#[allow(dead_code)]
impl <T> Point3<T> where T : Clone {
    pub fn new_uniform(i: T) -> Self {
        Self{x: i.clone(), y: i.clone(), z: i}
    }
}

#[allow(dead_code)]
impl Point3<i32> {
    pub fn to_f32(self) -> Point3<f32> {
        Point3::new(self.x as f32, self.y as f32, self.z as f32)
    }
}
impl Point3<i64> {
    pub fn to_f32(self) -> Point3<f32> {
        Point3::new(self.x as f32, self.y as f32, self.z as f32)
    }
}
#[allow(dead_code)]
impl Point3<f32> {
    pub fn add(self, other: Self) -> Self {
        self.apply_other(other, |s, o| s + o)
    }
    pub fn sub(self, other: Self) -> Self {
        self.apply_other(other, |s, o| s - o)
    }
    pub fn mul(self, other: Self) -> Self {
        self.apply_other(other, |s, o| s * o)
    }
    pub fn square(self) -> Self {
        self.apply(|v| v * v)
    }
    pub fn sum(&self) -> f32 {
        self.x + self. y + self.z
    }
}

impl <T> Default for Point3<T> where T: Default {
    fn default() -> Self {
        Self {
            x: T::default(),
            y: T::default(),
            z: T::default(),
        }
    }
}

impl<T> Display for Point3<T> where T: Display {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_str(&format!("X{} Y{} Z{}", self.x, self.y, self.z).to_string())?;
        Ok(())
    }
}

#[derive(Debug, PartialEq)]
pub struct ParsePoint3Error;
impl<T> FromStr for Point3<T> where T: FromStr, T: Default {
    type Err = ParsePoint3Error;
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let mut result:Point3<T> = Default::default();
        let mut xyz_set = [false, false, false];
        for part in input.split(' ') {
            if part.len() < 2 {
                continue;
            }
            let (id, nstr) = part.split_at(1);
            let num = nstr.parse::<T>();
            match (id, num) {
                ("X", Ok(x)) => { result.x = x; xyz_set[0] = true; },
                ("Y", Ok(y)) => { result.y = y; xyz_set[1] = true; },
                ("Z", Ok(z)) => { result.z = z; xyz_set[2] = true; },
                _ => { return Err(ParsePoint3Error{}); },
            };
        }
        if !xyz_set.iter().all(|&v| v) {
            warn!("not all values were set. {}, {}, {}", xyz_set[0], xyz_set[1], xyz_set[2]);
            return Err(ParsePoint3Error{}); 
        }
        Ok(result)
    }
}

pub trait DelayUpdates {
    fn needs_update(&self) -> bool;
    fn update(&mut self);
    fn update_check(&mut self) -> bool;
}

pub trait TrackCurrentPrevious<T> {
    fn current(&self) -> &T;
    fn previous(&self) -> &T;
    fn current_mut(&mut self) -> &mut T;
    fn previous_mut(&mut self) -> &mut T;
}

pub struct DiffTracker<T> {
    current: T,
    previous: T,
}

impl<T> DiffTracker<T> where T : Clone {
    pub fn new(initial: T) -> Self {
        Self {
            current: initial.clone(),
            previous: initial,
        }
    }
}

impl<T> TrackCurrentPrevious<T> for DiffTracker<T> {
    fn current(&self) -> &T { &self.current }
    fn previous(&self) -> &T { &self.previous }
    fn current_mut(&mut self) -> &mut T { &mut self.current }
    fn previous_mut(&mut self) -> &mut T { &mut self.previous }
}

impl<T> DelayUpdates for DiffTracker<T> where T : PartialEq, T : Clone {
    fn needs_update(&self) -> bool { self.current != self.previous }
    fn update(&mut self) { self.previous = self.current.clone(); }
    fn update_check(&mut self) -> bool {
        if self.needs_update() {
            self.update();
            true
        }
        else {
            false
        }
    }
}

pub struct DebounceTracker {
    update_time: Instant,
    debounce: Duration,
}

impl DebounceTracker {
    fn new(debounce: Duration) -> Self {
        Self { update_time: Instant::now(), debounce }
    }
}

impl DelayUpdates for DebounceTracker {
    fn needs_update(&self) -> bool { Instant::now() >= self.update_time + self.debounce }
    fn update(&mut self) { self.update_time = Instant::now(); }
    fn update_check(&mut self) -> bool {
        let now = Instant::now();
        if now >= self.update_time + self.debounce  {
            self.update_time = now;
            true
        }
        else {
            false
        }
    }
}

pub struct DebounceDiffTracker<T> {
    diff: DiffTracker<T>,
    debounce: DebounceTracker,
}

impl<T> DebounceDiffTracker<T> where T : Clone {
    pub fn new(initial: T, debounce: Duration) -> Self {
        Self {
            diff: DiffTracker::<T>::new(initial),
            debounce: DebounceTracker::new(debounce),
        }
    }
}

impl<T> TrackCurrentPrevious<T> for DebounceDiffTracker<T> {
    fn current(&self) -> &T { &self.diff.current() }
    fn previous(&self) -> &T { &self.diff.previous() }
    fn current_mut(&mut self) -> &mut T { self.diff.current_mut() }
    fn previous_mut(&mut self) -> &mut T { self.diff.previous_mut() }
}

impl<T> DelayUpdates for DebounceDiffTracker<T> where T : Clone, T : PartialEq {
    fn needs_update(&self) -> bool { self.diff.needs_update() && self.debounce.needs_update() }
    fn update(&mut self) { self.diff.update(); self.debounce.update(); }
    fn update_check(&mut self) -> bool {
        if self.diff.needs_update() {
            if self.debounce.update_check() {
                self.diff.update();
                return true;
            }
        }
        return false;
    }
}

#[allow(dead_code)]
#[derive(Clone)]
pub enum RemoteEvent {
    DialXYZEvent(Point3<i64>),
    SDList((String, usize)),
    SDLoadFile(String),
    RunGCode(String),
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParseCNCEventError;
impl FromStr for CncEvent {
    type Err = ParseCNCEventError;
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        if input.to_lowercase().contains("ok") {
            Ok(CncEvent::Ok)
        }
        else {
            warn!("unrecognized input: {}", input);
            // todo
            Ok(CncEvent::Unknown)
        }
    }
}

#[allow(dead_code)]
#[derive(Clone)]
pub enum CncEvent {
    Unknown,
    Ok,
    PositionReport(String), // from M114\n // X:0.00 Y:127.00 Z:145.00 E:0.00 Count X: 0 Y:10160 Z:116000\n // ok \n
    EndStopStates(String), // from M119\n // x_min: open\n y_min: open\nz_min: TRIGGERED\nz_probe: open\nfilament: open\n
}

#[allow(dead_code)]
#[derive(Clone, PartialEq, Eq)]
pub enum AppMode {
    Uninitialized,
    Jog,
    RunningFile,
}
