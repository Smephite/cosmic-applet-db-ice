use cosmic::iced::futures::SinkExt;
use cosmic::iced::widget::{canvas, column, row};
use cosmic::iced::{self, mouse, Alignment, Color, Length, Point, Subscription};
use cosmic::widget::{autosize, button, container, divider, space, text, Id};
use cosmic::{app, Element, Task};
use serde::Deserialize;
use std::collections::VecDeque;
use std::process::Command;
use std::time::{Duration, Instant};

const STATUS_URL: &str = "https://iceportal.de/api1/rs/status";
const TRIP_URL: &str = "https://iceportal.de/api1/rs/tripInfo/trip";
const POLL_INTERVAL: Duration = Duration::from_secs(1);
const WIFI_CHECK_INTERVAL: Duration = Duration::from_secs(5);
const TRIP_POLL_INTERVAL: Duration = Duration::from_secs(15);
const SPEED_HISTORY_DURATION: Duration = Duration::from_secs(15 * 60);

const APP_ID: &str = "dev.smephite.CosmicAppletDbIce";
const ICE_SSIDS: &[&str] = &["WIFIonICE", "WIFI@DB"];

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct IceStatus {
    speed: Option<f64>,
    train_type: Option<String>,
    internet: Option<String>,
    #[serde(default)]
    connection: bool,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct TripInfo {
    trip: Option<TripData>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct TripData {
    vzn: Option<String>,
    train_type: Option<String>,
    stops: Option<Vec<Stop>>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct Stop {
    station: Option<Station>,
    info: Option<StopInfo>,
    timetable: Option<Timetable>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct Station {
    name: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct StopInfo {
    passed: Option<bool>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct Timetable {
    arrival_delay: Option<String>,
}

fn is_on_ice_wifi() -> bool {
    Command::new("nmcli")
        .args(["-t", "-f", "active,ssid", "dev", "wifi"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .is_some_and(|out| {
            out.lines()
                .any(|line| line.strip_prefix("yes:").is_some_and(|ssid| ICE_SSIDS.contains(&ssid)))
        })
}

fn fetch_json<T: serde::de::DeserializeOwned>(url: &str) -> Option<T> {
    let body: String = ureq::get(url)
        .call()
        .ok()?
        .into_body()
        .read_to_string()
        .ok()?;
    serde_json::from_str(&body).ok()
}

struct SpeedGraph {
    samples: VecDeque<(Instant, f64)>,
}

impl SpeedGraph {
    fn new() -> Self {
        Self {
            samples: VecDeque::new(),
        }
    }

    fn push(&mut self, speed: f64) {
        let now = Instant::now();
        self.samples.push_back((now, speed));
        let cutoff = now - SPEED_HISTORY_DURATION;
        while self.samples.front().is_some_and(|s| s.0 < cutoff) {
            self.samples.pop_front();
        }
    }

    fn max_speed(&self) -> f64 {
        self.samples
            .iter()
            .map(|s| s.1)
            .fold(0.0f64, f64::max)
            .max(10.0)
    }
}

impl<Message> canvas::Program<Message, cosmic::Theme, cosmic::Renderer> for SpeedGraph {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &cosmic::Renderer,
        _theme: &cosmic::Theme,
        bounds: iced::Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry<cosmic::Renderer>> {
        if self.samples.len() < 2 {
            return vec![];
        }

        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let w = bounds.width;
        let h = bounds.height;
        let max_speed = self.max_speed();
        let now = Instant::now();
        let window = SPEED_HISTORY_DURATION.as_secs_f64();

        let grid_color = Color::from_rgba(1.0, 1.0, 1.0, 0.1);
        for i in 1..4 {
            let y = h * (i as f32) / 4.0;
            let mut line = canvas::path::Builder::new();
            line.move_to(Point::new(0.0, y));
            line.line_to(Point::new(w, y));
            frame.stroke(
                &line.build(),
                canvas::Stroke::default()
                    .with_color(grid_color)
                    .with_width(1.0),
            );
        }

        let mut path = canvas::path::Builder::new();
        for (i, (t, speed)) in self.samples.iter().enumerate() {
            let age = now.duration_since(*t).as_secs_f64();
            let x = w * (1.0 - age as f32 / window as f32);
            let y = h * (1.0 - *speed as f32 / max_speed as f32);
            if i == 0 {
                path.move_to(Point::new(x, y));
            } else {
                path.line_to(Point::new(x, y));
            }
        }

        frame.stroke(
            &path.build(),
            canvas::Stroke::default()
                .with_color(Color::from_rgba(0.4, 0.7, 1.0, 0.9))
                .with_width(2.0),
        );

        vec![frame.into_geometry()]
    }
}

fn format_delay(delay: Option<&str>) -> String {
    match delay {
        Some(d) if !d.is_empty() && d != "0" => format!(" ({d} min)"),
        _ => String::new(),
    }
}

struct IceApplet {
    core: app::Core,
    on_ice_wifi: bool,
    status: Option<IceStatus>,
    trip: Option<TripInfo>,
    popup: Option<cosmic::iced::window::Id>,
    speed_graph: SpeedGraph,
}

#[derive(Debug, Clone)]
enum Message {
    WifiCheck(bool),
    StatusUpdate(Option<IceStatus>),
    TripUpdate(Option<TripInfo>),
    TogglePopup,
    CloseRequested(cosmic::iced::window::Id),
}

impl IceApplet {
    fn next_stop(&self) -> Option<(&str, Option<&str>)> {
        let stops = self.trip.as_ref()?.trip.as_ref()?.stops.as_ref()?;
        let next = stops.iter().find(|s| {
            s.info
                .as_ref()
                .and_then(|i| i.passed)
                .is_some_and(|p| !p)
        })?;
        let name = next.station.as_ref()?.name.as_deref()?;
        let delay = next
            .timetable
            .as_ref()
            .and_then(|t| t.arrival_delay.as_deref());
        Some((name, delay))
    }

    fn destination(&self) -> Option<&str> {
        let stops = self.trip.as_ref()?.trip.as_ref()?.stops.as_ref()?;
        stops.last()?.station.as_ref()?.name.as_deref()
    }

    fn train_number(&self) -> Option<String> {
        let trip = self.trip.as_ref()?.trip.as_ref()?;
        let train_type = trip
            .train_type
            .as_deref()
            .or(self.status.as_ref().and_then(|s| s.train_type.as_deref()))?;
        let vzn = trip.vzn.as_deref()?;
        Some(format!("{train_type} {vzn}"))
    }

    fn remaining_stops(&self) -> Vec<(&str, String)> {
        let Some(stops) = self
            .trip
            .as_ref()
            .and_then(|t| t.trip.as_ref())
            .and_then(|t| t.stops.as_ref())
        else {
            return vec![];
        };

        stops
            .iter()
            .filter(|s| {
                s.info
                    .as_ref()
                    .and_then(|i| i.passed)
                    .is_some_and(|p| !p)
            })
            .skip(1) // skip next stop (shown separately)
            .filter_map(|s| {
                let name = s.station.as_ref()?.name.as_deref()?;
                let delay = format_delay(
                    s.timetable
                        .as_ref()
                        .and_then(|t| t.arrival_delay.as_deref()),
                );
                Some((name, delay))
            })
            .collect()
    }
}

impl cosmic::Application for IceApplet {
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    const APP_ID: &'static str = APP_ID;

    fn init(core: app::Core, _flags: ()) -> (Self, Task<cosmic::Action<Message>>) {
        (
            Self {
                core,
                on_ice_wifi: false,
                status: None,
                trip: None,
                popup: None,
                speed_graph: SpeedGraph::new(),
            },
            Task::none(),
        )
    }

    fn core(&self) -> &app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut app::Core {
        &mut self.core
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }

    fn subscription(&self) -> Subscription<Message> {
        let wifi_sub = Subscription::run(|| {
            cosmic::iced::stream::channel(
                1,
                |mut output: cosmic::iced::futures::channel::mpsc::Sender<Message>| async move {
                    loop {
                        let on_ice = tokio::task::spawn_blocking(is_on_ice_wifi)
                            .await
                            .unwrap_or(false);
                        let _ = output.send(Message::WifiCheck(on_ice)).await;
                        tokio::time::sleep(WIFI_CHECK_INTERVAL).await;
                    }
                },
            )
        });

        let mut subs = vec![wifi_sub];

        if self.on_ice_wifi {
            subs.push(Subscription::run(|| {
                cosmic::iced::stream::channel(
                    1,
                    |mut output: cosmic::iced::futures::channel::mpsc::Sender<Message>| async move {
                        loop {
                            let status = tokio::task::spawn_blocking(|| fetch_json(STATUS_URL))
                                .await
                                .ok()
                                .flatten();
                            let _ = output.send(Message::StatusUpdate(status)).await;
                            tokio::time::sleep(POLL_INTERVAL).await;
                        }
                    },
                )
            }));

            subs.push(Subscription::run(|| {
                cosmic::iced::stream::channel(
                    1,
                    |mut output: cosmic::iced::futures::channel::mpsc::Sender<Message>| async move {
                        loop {
                            let trip = tokio::task::spawn_blocking(|| fetch_json(TRIP_URL))
                                .await
                                .ok()
                                .flatten();
                            let _ = output.send(Message::TripUpdate(trip)).await;
                            tokio::time::sleep(TRIP_POLL_INTERVAL).await;
                        }
                    },
                )
            }));
        }

        Subscription::batch(subs)
    }

    fn update(&mut self, message: Message) -> Task<cosmic::Action<Message>> {
        match message {
            Message::WifiCheck(on_ice) => {
                if self.on_ice_wifi != on_ice {
                    self.on_ice_wifi = on_ice;
                    if !on_ice {
                        self.status = None;
                        self.trip = None;
                    }
                }
            }
            Message::StatusUpdate(status) => {
                if let Some(s) = &status {
                    if s.connection {
                        self.speed_graph.push(s.speed.unwrap_or(0.0));
                    }
                }
                self.status = status;
            }
            Message::TripUpdate(trip) => {
                self.trip = trip;
            }
            Message::TogglePopup => {
                return if let Some(id) = self.popup.take() {
                    iced_winit::commands::popup::destroy_popup(id)
                } else {
                    let new_id = cosmic::iced::window::Id::unique();
                    self.popup = Some(new_id);
                    let settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        Some((320, 400)),
                        None,
                        None,
                    );
                    iced_winit::commands::popup::get_popup(settings)
                };
            }
            Message::CloseRequested(id) => {
                if Some(id) == self.popup {
                    self.popup = None;
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        if !self.on_ice_wifi {
            return container(column![]).width(0).height(0).into();
        }

        let label = match &self.status {
            Some(s) if s.connection => {
                let speed = s
                    .speed
                    .map(|v| format!("{} km/h", v as u32))
                    .unwrap_or_else(|| "-- km/h".into());
                let train = s.train_type.as_deref().unwrap_or("ICE");
                format!("{train} {speed}")
            }
            _ => "ICE: --".into(),
        };

        let pad = self.core.applet.suggested_padding(true).0;
        let txt = self
            .core
            .applet
            .text(label)
            .wrapping(cosmic::iced::widget::text::Wrapping::None);

        let btn = button::custom(container(txt).padding([0, pad]))
            .on_press_down(Message::TogglePopup)
            .class(cosmic::theme::Button::AppletIcon);

        autosize::autosize(btn, Id::new("ice-main")).into()
    }

    fn view_window(&self, _id: cosmic::iced::window::Id) -> Element<'_, Message> {
        let mut items: Vec<Element<Message>> = Vec::new();

        if let Some(train) = self.train_number() {
            let dest = self.destination().unwrap_or("?");
            items.push(text::title4(format!("{train} -> {dest}")).into());
            items.push(divider::horizontal::default().into());
        }

        if let Some(s) = &self.status {
            let speed = s
                .speed
                .map(|v| format!("{} km/h", v as u32))
                .unwrap_or_else(|| "-- km/h".into());
            items.push(
                row![text::body("Speed"), space::horizontal(), text::body(speed)]
                    .align_y(Alignment::Center)
                    .into(),
            );

            let inet = s.internet.as_deref().unwrap_or("--");
            items.push(
                row![text::body("Internet"), space::horizontal(), text::body(inet)]
                    .align_y(Alignment::Center)
                    .into(),
            );
        }

        if self.speed_graph.samples.len() >= 2 {
            let max = self.speed_graph.max_speed() as u32;
            items.push(divider::horizontal::default().into());
            items.push(text::body(format!("Speed (15 min, max {max} km/h)")).into());
            items.push(
                canvas::Canvas::new(&self.speed_graph)
                    .width(Length::Fill)
                    .height(Length::Fixed(80.0))
                    .into(),
            );
        }

        if let Some((name, delay)) = self.next_stop() {
            items.push(divider::horizontal::default().into());
            items.push(
                row![
                    text::body("Next stop"),
                    space::horizontal(),
                    text::body(format!("{name}{}", format_delay(delay)))
                ]
                .align_y(Alignment::Center)
                .into(),
            );
        }

        let remaining = self.remaining_stops();
        if !remaining.is_empty() {
            items.push(divider::horizontal::default().into());
            for (name, delay) in &remaining {
                items.push(text::body(format!("  {name}{delay}")).into());
            }
        }

        if items.is_empty() {
            items.push(text::body("Not connected to ICE WiFi").into());
        }

        let content = column(items).spacing(8).padding(16);
        self.core
            .applet
            .popup_container(container(content))
            .max_width(350.)
            .into()
    }

    fn on_close_requested(&self, id: cosmic::iced::window::Id) -> Option<Message> {
        Some(Message::CloseRequested(id))
    }
}

fn main() -> cosmic::iced::Result {
    cosmic::applet::run::<IceApplet>(())
}
