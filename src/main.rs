use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{Duration as ChronoDuration, Local};
use native_tls::{Protocol, TlsConnector};
use openaction::*;
use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, QoS, TlsConfiguration, Transport};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    sync::{mpsc, watch, Mutex},
    task::JoinHandle,
    time::{interval, sleep, MissedTickBehavior},
};

const ACTION_UUID: &str = "de.spliter90.bambu.status";

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
struct BambuSettings {
    host: String,
    serial: String,
    access_code: String,
    display_mode: String,
    pushall_interval_secs: u64,
}

impl Default for BambuSettings {
    fn default() -> Self {
        Self {
            host: String::new(),
            serial: String::new(),
            access_code: String::new(),
            display_mode: "remaining".to_owned(),
            pushall_interval_secs: 30,
        }
    }
}

impl BambuSettings {
    fn valid(&self) -> bool {
        !self.host.trim().is_empty()
            && !self.serial.trim().is_empty()
            && !self.access_code.trim().is_empty()
    }

    fn printer_key(&self) -> PrinterKey {
        PrinterKey {
            host: self.host.trim().to_owned(),
            serial: self.serial.trim().to_ascii_uppercase(),
            access_code: self.access_code.trim().to_owned(),
            pushall_interval_secs: self.pushall_interval_secs.clamp(10, 300),
        }
    }
}

#[derive(Clone, Eq)]
struct PrinterKey {
    host: String,
    serial: String,
    access_code: String,
    pushall_interval_secs: u64,
}

impl PartialEq for PrinterKey {
    fn eq(&self, other: &Self) -> bool {
        self.host == other.host
            && self.serial == other.serial
            && self.access_code == other.access_code
            && self.pushall_interval_secs == other.pushall_interval_secs
    }
}

impl Hash for PrinterKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.host.hash(state);
        self.serial.hash(state);
        self.access_code.hash(state);
        self.pushall_interval_secs.hash(state);
    }
}

#[derive(Clone, Debug, Default)]
struct PrinterSnapshot {
    connected: bool,
    gcode_state: Option<String>,
    percent: Option<u8>,
    remaining_minutes: Option<i64>,
    print_error: Option<i64>,
    layer: Option<u64>,
    total_layers: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrinterCommand {
    Pause,
    Resume,
}

struct ConnectionEntry {
    sender: watch::Sender<PrinterSnapshot>,
    command_tx: mpsc::Sender<PrinterCommand>,
    refs: usize,
    task: JoinHandle<()>,
}

struct BindingEntry {
    key: PrinterKey,
    render_task: JoinHandle<()>,
}

#[derive(Default)]
struct SharedState {
    connections: Mutex<HashMap<PrinterKey, ConnectionEntry>>,
    bindings: Mutex<HashMap<String, BindingEntry>>,
    presses: Mutex<HashMap<String, Instant>>,
}

impl SharedState {
    async fn bind(&self, instance: &Instance, settings: &BambuSettings) -> OpenActionResult<()> {
        self.unbind(&instance.instance_id).await;

        if !settings.valid() {
            instance
                .set_image(Some(solid_background_data_url("#424242")), None)
                .await?;
            instance.set_title(Some("SETUP\nFEHLT"), None).await?;
            return Ok(());
        }

        instance
            .set_image(Some(solid_background_data_url("#424242")), None)
            .await?;
        instance.set_title(Some("VERBINDE..."), None).await?;

        let key = settings.printer_key();
        let mut receiver = {
            let mut connections = self.connections.lock().await;
            if let Some(entry) = connections.get_mut(&key) {
                entry.refs += 1;
                entry.sender.subscribe()
            } else {
                let (sender, receiver) = watch::channel(PrinterSnapshot::default());
                let (command_tx, command_rx) = mpsc::channel(8);
                let worker_sender = sender.clone();
                let worker_key = key.clone();
                let task = tokio::spawn(async move {
                    printer_worker(worker_key, worker_sender, command_rx).await;
                });
                connections.insert(
                    key.clone(),
                    ConnectionEntry {
                        sender,
                        command_tx,
                        refs: 1,
                        task,
                    },
                );
                receiver
            }
        };

        let instance_id = instance.instance_id.clone();
        let display_mode = settings.display_mode.clone();
        let render_id = instance_id.clone();
        let render_task = tokio::spawn(async move {
            let snapshot = receiver.borrow().clone();
            render_instance(&render_id, &display_mode, snapshot).await;
            loop {
                if receiver.changed().await.is_err() {
                    break;
                }
                let snapshot = receiver.borrow().clone();
            render_instance(&render_id, &display_mode, snapshot).await;
            }
        });

        self.bindings.lock().await.insert(
            instance_id,
            BindingEntry {
                key,
                render_task,
            },
        );

        Ok(())
    }

    async fn unbind(&self, instance_id: &str) {
        let binding = self.bindings.lock().await.remove(instance_id);
        let Some(binding) = binding else {
            return;
        };

        binding.render_task.abort();

        let mut connections = self.connections.lock().await;
        let should_remove = if let Some(entry) = connections.get_mut(&binding.key) {
            entry.refs = entry.refs.saturating_sub(1);
            entry.refs == 0
        } else {
            false
        };

        if should_remove {
            if let Some(entry) = connections.remove(&binding.key) {
                entry.task.abort();
            }
        }
    }

    async fn press_down(&self, instance_id: &str) {
        self.presses
            .lock()
            .await
            .insert(instance_id.to_owned(), Instant::now());
    }

    async fn press_duration(&self, instance_id: &str) -> Duration {
        self.presses
            .lock()
            .await
            .remove(instance_id)
            .map(|started| started.elapsed())
            .unwrap_or_default()
    }

    async fn send_control(&self, instance_id: &str, command: PrinterCommand) -> bool {
        let key = {
            let bindings = self.bindings.lock().await;
            let Some(binding) = bindings.get(instance_id) else {
                return false;
            };
            binding.key.clone()
        };

        let (snapshot, tx) = {
            let connections = self.connections.lock().await;
            let Some(entry) = connections.get(&key) else {
                return false;
            };
            (entry.sender.borrow().clone(), entry.command_tx.clone())
        };

        if !control_allowed(&snapshot, command) {
            return false;
        }

        tx.send(command).await.is_ok()
    }
}

struct BambuStatusAction {
    shared: Arc<SharedState>,
}

#[async_trait]
impl Action for BambuStatusAction {
    const UUID: ActionUuid = ACTION_UUID;
    type Settings = BambuSettings;

    async fn will_appear(
        &self,
        instance: &Instance,
        settings: &Self::Settings,
    ) -> OpenActionResult<()> {
        self.shared.bind(instance, settings).await
    }

    async fn did_receive_settings(
        &self,
        instance: &Instance,
        settings: &Self::Settings,
    ) -> OpenActionResult<()> {
        self.shared.bind(instance, settings).await
    }

    async fn key_down(
        &self,
        instance: &Instance,
        _settings: &Self::Settings,
    ) -> OpenActionResult<()> {
        self.shared.press_down(&instance.instance_id).await;
        Ok(())
    }

    async fn key_up(
        &self,
        instance: &Instance,
        _settings: &Self::Settings,
    ) -> OpenActionResult<()> {
        let held = self.shared.press_duration(&instance.instance_id).await;
        let command = if held >= Duration::from_secs(2) {
            PrinterCommand::Resume
        } else {
            PrinterCommand::Pause
        };

        if self.shared.send_control(&instance.instance_id, command).await {
            instance.show_ok().await?;
        } else {
            instance.show_alert().await?;
        }

        Ok(())
    }

    async fn will_disappear(
        &self,
        instance: &Instance,
        _settings: &Self::Settings,
    ) -> OpenActionResult<()> {
        self.shared.unbind(&instance.instance_id).await;
        self.shared.presses.lock().await.remove(&instance.instance_id);
        Ok(())
    }
}

async fn render_instance(instance_id: &str, display_mode: &str, snapshot: PrinterSnapshot) {
    let Some(instance) = get_instance(instance_id.to_owned()).await else {
        return;
    };

    let title = format_title(&snapshot, display_mode);
    let image = background_image_data_url(&snapshot);

    if let Err(error) = instance.set_image(Some(image), None).await {
        log::warn!("Could not update Bambu status image: {error}");
    }
    if let Err(error) = instance.set_title(Some(title), None).await {
        log::warn!("Could not update Bambu status title: {error}");
    }
}

fn format_title(snapshot: &PrinterSnapshot, display_mode: &str) -> String {
    if !snapshot.connected {
        return "OFFLINE".to_owned();
    }

    let raw_state = snapshot
        .gcode_state
        .as_deref()
        .unwrap_or("ONLINE")
        .to_ascii_uppercase();

    let state = match raw_state.as_str() {
        "RUNNING" => "DRUCKT",
        "PAUSE" | "PAUSED" => "PAUSE",
        "FINISH" | "FINISHED" => "FERTIG",
        "FAILED" | "FAIL" => "FEHLER",
        "IDLE" => "BEREIT",
        "PREPARE" | "PREPARING" => "STARTET",
        "SLICING" => "SLICING",
        other => other,
    };

    if state == "FEHLER" {
        if let Some(code) = snapshot.print_error.filter(|code| *code != 0) {
            return format!("FEHLER\n{code}");
        }
        return "FEHLER".to_owned();
    }

    if matches!(state, "BEREIT" | "FERTIG") {
        if state == "FERTIG" {
            if let Some(percent) = snapshot.percent {
                return format!("FERTIG\n{} %", percent.min(100));
            }
        }
        return state.to_owned();
    }

    let mut lines = vec![state.to_owned()];

    if let Some(percent) = snapshot.percent {
        lines.push(format!("{} %", percent.min(100)));
    } else if let (Some(layer), Some(total)) = (snapshot.layer, snapshot.total_layers) {
        if total > 0 {
            lines.push(format!("L {layer}/{total}"));
        }
    }

    if let Some(minutes) = snapshot.remaining_minutes.filter(|m| *m >= 0) {
        if display_mode == "finish" {
            let finish = Local::now() + ChronoDuration::minutes(minutes);
            lines.push(format!("Fertig {}", finish.format("%H:%M")));
        } else {
            lines.push(format_duration(minutes));
        }
    }

    lines.truncate(3);
    lines.join("\n")
}

fn control_allowed(snapshot: &PrinterSnapshot, command: PrinterCommand) -> bool {
    if !snapshot.connected {
        return false;
    }

    let state = snapshot
        .gcode_state
        .as_deref()
        .unwrap_or("")
        .to_ascii_uppercase();

    match command {
        PrinterCommand::Pause => state == "RUNNING",
        PrinterCommand::Resume => matches!(state.as_str(), "PAUSE" | "PAUSED"),
    }
}

fn background_color(snapshot: &PrinterSnapshot) -> &'static str {
    if !snapshot.connected {
        return "#424242";
    }

    if snapshot.print_error.unwrap_or(0) != 0 {
        return "#C62828";
    }

    let state = snapshot
        .gcode_state
        .as_deref()
        .unwrap_or("ONLINE")
        .to_ascii_uppercase();

    match state.as_str() {
        "FAILED" | "FAIL" => "#C62828",
        "FINISH" | "FINISHED" => "#1565C0",
        "IDLE" => "#2E7D32",
        "RUNNING" | "PAUSE" | "PAUSED" | "PREPARE" | "PREPARING" | "SLICING" => "#D4A017",
        _ => "#424242",
    }
}

fn solid_background_data_url(color: &str) -> String {
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="144" height="144" viewBox="0 0 144 144"><rect width="144" height="144" rx="18" fill="{color}"/><rect x="5" y="5" width="134" height="134" rx="14" fill="none" stroke="white" stroke-opacity=".16" stroke-width="2"/></svg>"#
    );
    format!("data:image/svg+xml;base64,{}", BASE64.encode(svg.as_bytes()))
}

fn background_image_data_url(snapshot: &PrinterSnapshot) -> String {
    solid_background_data_url(background_color(snapshot))
}

fn format_duration(minutes: i64) -> String {
    let minutes = minutes.max(0);
    let hours = minutes / 60;
    let mins = minutes % 60;
    if hours > 0 {
        format!("{hours}h {mins:02}m")
    } else {
        format!("{mins} min")
    }
}

async fn printer_worker(
    key: PrinterKey,
    sender: watch::Sender<PrinterSnapshot>,
    mut command_rx: mpsc::Receiver<PrinterCommand>,
) {
    loop {
        sender.send_modify(|snapshot| snapshot.connected = false);

        if let Err(error) = run_mqtt_session(&key, &sender, &mut command_rx).await {
            log::warn!(
                "Bambu MQTT disconnected for {} at {}: {}",
                key.serial,
                key.host,
                error
            );
        }

        sender.send_modify(|snapshot| snapshot.connected = false);
        sleep(Duration::from_secs(5)).await;
    }
}

async fn run_mqtt_session(
    key: &PrinterKey,
    sender: &watch::Sender<PrinterSnapshot>,
    command_rx: &mut mpsc::Receiver<PrinterCommand>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut tls_builder = TlsConnector::builder();
    tls_builder
        .min_protocol_version(Some(Protocol::Tlsv12))
        .max_protocol_version(Some(Protocol::Tlsv12))
        .danger_accept_invalid_certs(true)
        .danger_accept_invalid_hostnames(true)
        .use_sni(false);
    let tls_connector = tls_builder.build()?;

    let client_id = format!("opendeck-bambu-{}", short_serial(&key.serial));
    let mut options = MqttOptions::new(client_id, key.host.clone(), 8883);
    options.set_keep_alive(Duration::from_secs(30));
    options.set_clean_session(true);
    options.set_credentials("bblp", key.access_code.clone());
    options.set_transport(Transport::tls_with_config(
        TlsConfiguration::NativeConnector(tls_connector),
    ));

    let (client, mut eventloop) = AsyncClient::new(options, 20);
    let report_topic = format!("device/{}/report", key.serial);
    let request_topic = format!("device/{}/request", key.serial);

    client.subscribe(report_topic, QoS::AtMostOnce).await?;

    let mut refresh = interval(Duration::from_secs(key.pushall_interval_secs));
    refresh.set_missed_tick_behavior(MissedTickBehavior::Skip);
    refresh.tick().await;

    loop {
        tokio::select! {
            _ = refresh.tick() => {
                request_pushall(&client, &request_topic).await?;
            }
            command = command_rx.recv() => {
                if let Some(command) = command {
                    send_print_control(&client, &request_topic, command).await?;
                }
            }
            event = eventloop.poll() => {
                match event? {
                    Event::Incoming(Incoming::ConnAck(_)) => {
                        sender.send_modify(|snapshot| snapshot.connected = true);
                        request_pushall(&client, &request_topic).await?;
                    }
                    Event::Incoming(Incoming::Publish(publish)) => {
                        if let Ok(value) = serde_json::from_slice::<Value>(&publish.payload) {
                            sender.send_modify(|snapshot| {
                                snapshot.connected = true;
                                merge_report(snapshot, &value);
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

async fn send_print_control(
    client: &AsyncClient,
    request_topic: &str,
    command: PrinterCommand,
) -> Result<(), rumqttc::ClientError> {
    let payload: &'static [u8] = match command {
        PrinterCommand::Pause => br#"{"print":{"sequence_id":"0","command":"pause"}}"#,
        PrinterCommand::Resume => br#"{"print":{"sequence_id":"0","command":"resume"}}"#,
    };

    client
        .publish(request_topic, QoS::AtMostOnce, false, payload)
        .await
}

async fn request_pushall(
    client: &AsyncClient,
    request_topic: &str,
) -> Result<(), rumqttc::ClientError> {
    let payload = br#"{"pushing":{"sequence_id":"0","command":"pushall","version":1,"push_target":1}}"#;
    client
        .publish(request_topic, QoS::AtMostOnce, false, payload.as_slice())
        .await
}

fn merge_report(snapshot: &mut PrinterSnapshot, value: &Value) {
    let Some(print) = value.get("print").and_then(Value::as_object) else {
        return;
    };

    if let Some(state) = print.get("gcode_state").and_then(Value::as_str) {
        snapshot.gcode_state = Some(state.to_owned());
    }
    if let Some(percent) = number_i64(print.get("mc_percent")) {
        snapshot.percent = Some(percent.clamp(0, 100) as u8);
    }
    if let Some(remaining) = number_i64(print.get("mc_remaining_time")) {
        snapshot.remaining_minutes = Some(remaining.max(0));
    }
    if let Some(error) = number_i64(print.get("print_error")) {
        snapshot.print_error = Some(error);
    }
    if let Some(layer) = number_u64(print.get("layer_num")) {
        snapshot.layer = Some(layer);
    }
    if let Some(total) = number_u64(print.get("total_layer_num")) {
        snapshot.total_layers = Some(total);
    }
}

fn number_i64(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|v| i64::try_from(v).ok()))
        .or_else(|| value.as_str().and_then(|v| v.parse::<i64>().ok()))
}

fn number_u64(value: Option<&Value>) -> Option<u64> {
    let value = value?;
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|v| u64::try_from(v).ok()))
        .or_else(|| value.as_str().and_then(|v| v.parse::<u64>().ok()))
}

fn short_serial(serial: &str) -> String {
    let filtered: String = serial
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(12)
        .collect();
    if filtered.is_empty() {
        "printer".to_owned()
    } else {
        filtered
    }
}

#[tokio::main]
async fn main() -> OpenActionResult<()> {
    use simplelog::*;
    if let Err(error) = TermLogger::init(
        LevelFilter::Info,
        Config::default(),
        TerminalMode::Stdout,
        ColorChoice::Never,
    ) {
        eprintln!("Logger initialization failed: {error}");
    }

    let shared = Arc::new(SharedState::default());
    register_action(BambuStatusAction { shared }).await;
    run(std::env::args().collect()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_remaining_time() {
        assert_eq!(format_duration(0), "0 min");
        assert_eq!(format_duration(59), "59 min");
        assert_eq!(format_duration(84), "1h 24m");
    }

    #[test]
    fn merges_partial_status_reports() {
        let mut snapshot = PrinterSnapshot::default();
        let a: Value = serde_json::json!({
            "print": {"gcode_state": "RUNNING", "mc_percent": 67}
        });
        let b: Value = serde_json::json!({
            "print": {"mc_remaining_time": 84, "layer_num": 120, "total_layer_num": 300}
        });
        merge_report(&mut snapshot, &a);
        merge_report(&mut snapshot, &b);

        assert_eq!(snapshot.gcode_state.as_deref(), Some("RUNNING"));
        assert_eq!(snapshot.percent, Some(67));
        assert_eq!(snapshot.remaining_minutes, Some(84));
        assert_eq!(snapshot.layer, Some(120));
        assert_eq!(snapshot.total_layers, Some(300));
    }

    #[test]
    fn maps_status_colors() {
        let ready = PrinterSnapshot {
            connected: true,
            gcode_state: Some("IDLE".into()),
            ..Default::default()
        };
        let printing = PrinterSnapshot {
            connected: true,
            gcode_state: Some("RUNNING".into()),
            ..Default::default()
        };
        let finished = PrinterSnapshot {
            connected: true,
            gcode_state: Some("FINISH".into()),
            ..Default::default()
        };
        let error = PrinterSnapshot {
            connected: true,
            gcode_state: Some("FAILED".into()),
            ..Default::default()
        };

        assert_eq!(background_color(&ready), "#2E7D32");
        assert_eq!(background_color(&printing), "#D4A017");
        assert_eq!(background_color(&finished), "#1565C0");
        assert_eq!(background_color(&error), "#C62828");
    }

    #[test]
    fn validates_pause_resume_states() {
        let running = PrinterSnapshot {
            connected: true,
            gcode_state: Some("RUNNING".into()),
            ..Default::default()
        };
        let paused = PrinterSnapshot {
            connected: true,
            gcode_state: Some("PAUSE".into()),
            ..Default::default()
        };

        assert!(control_allowed(&running, PrinterCommand::Pause));
        assert!(!control_allowed(&running, PrinterCommand::Resume));
        assert!(control_allowed(&paused, PrinterCommand::Resume));
        assert!(!control_allowed(&paused, PrinterCommand::Pause));
    }

    #[test]
    fn formats_running_title() {
        let snapshot = PrinterSnapshot {
            connected: true,
            gcode_state: Some("RUNNING".into()),
            percent: Some(67),
            remaining_minutes: Some(84),
            ..Default::default()
        };
        assert_eq!(format_title(&snapshot, "remaining"), "DRUCKT\n67 %\n1h 24m");
    }
}
