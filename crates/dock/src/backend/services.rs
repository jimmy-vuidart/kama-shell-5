//! The switches and the brightness of the control center, through the tools
//! of each service: `nmcli` (NetworkManager), `bluetoothctl` (BlueZ) and
//! `ddcutil` (the screens, over DDC/CI). A tool that is missing leaves its
//! control greyed, and nothing else changes.

use std::process::Stdio;
use std::time::Duration;

use iced::Subscription;
use iced::futures::channel::mpsc::Sender;
use iced::futures::{SinkExt, Stream};
use tokio::process::Command;
use tokio::sync::mpsc::unbounded_channel;
use tracing::{debug, warn};

use super::{Backend, Request, Switch, Update};

/// The output of a tool, when it ran and succeeded.
async fn run(program: &str, arguments: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(arguments)
        .env("LC_ALL", "C")
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The network, from `nmcli networking` and
/// `nmcli -t -f TYPE,STATE,CONNECTION device`: on or off, and what it is
/// connected through.
pub fn parse_network(networking: &str, devices: &str) -> Switch {
    let on = networking.trim() == "enabled";
    let connected = devices.lines().filter_map(|line| {
        let mut fields = line.splitn(3, ':');
        let (kind, state, name) = (fields.next()?, fields.next()?, fields.next()?);
        (state == "connected").then_some((kind, name))
    });
    // A cable first, then Wi-Fi under the name of its network.
    let mut detail = None;
    for (kind, name) in connected {
        match kind {
            "ethernet" => {
                detail = Some("Ethernet".to_owned());
                break;
            }
            "wifi" if detail.is_none() => detail = Some(name.to_owned()),
            _ => {}
        }
    }
    let fallback = if on { "Déconnecté" } else { "Désactivé" };
    Switch {
        on,
        detail: detail.filter(|_| on).unwrap_or_else(|| fallback.to_owned()),
        available: true,
    }
}

/// Bluetooth, from `bluetoothctl show` and `bluetoothctl devices
/// Connected`: on or off, and the device connected.
pub fn parse_bluetooth(controller: &str, devices: &str) -> Switch {
    let on = controller.lines().any(|line| line.trim() == "Powered: yes");
    // `Device AA:BB:CC:DD:EE:FF Name of the device`.
    let names: Vec<&str> = devices
        .lines()
        .filter_map(|line| line.strip_prefix("Device ")?.split_once(' '))
        .map(|(_, name)| name)
        .collect();
    let detail = match (on, names.as_slice()) {
        (false, _) => "Désactivé".to_owned(),
        (true, []) => "Activé".to_owned(),
        (true, [name]) => (*name).to_owned(),
        (true, names) => format!("{} appareils", names.len()),
    };
    Switch {
        on,
        detail,
        available: controller.contains("Controller"),
    }
}

/// The brightness in the answer of `ddcutil getvcp 10 --brief`
/// (`VCP 10 C 70 100`: current value, then maximum), from 0 to 1.
pub fn parse_brightness(answer: &str) -> Option<f32> {
    let mut words = answer.split_whitespace().skip_while(|word| *word != "VCP");
    let (_vcp, _code, _kind) = (words.next()?, words.next()?, words.next()?);
    let current: f32 = words.next()?.parse().ok()?;
    let maximum: f32 = words.next()?.parse().ok()?;
    (maximum > 0.0).then(|| (current / maximum).clamp(0.0, 1.0))
}

/// The numbers of the screens `ddcutil detect --brief` can talk to.
pub fn parse_displays(answer: &str) -> Vec<u32> {
    let numbers = answer
        .lines()
        .filter_map(|line| line.strip_prefix("Display ")?.trim().parse().ok());
    numbers.collect()
}

async fn network() -> Switch {
    let networking = run("nmcli", &["networking"]).await;
    let devices = run("nmcli", &["-t", "-f", "TYPE,STATE,CONNECTION", "device"]).await;
    match (networking, devices) {
        (Some(networking), Some(devices)) => parse_network(&networking, &devices),
        _ => Switch::default(),
    }
}

async fn bluetooth() -> Switch {
    let Some(controller) = run("bluetoothctl", &["show"]).await else {
        return Switch::default();
    };
    let devices = run("bluetoothctl", &["devices", "Connected"])
        .await
        .unwrap_or_default();
    parse_bluetooth(&controller, &devices)
}

pub fn network_subscription() -> Subscription<Update> {
    Subscription::run(network_service)
}

fn network_service() -> impl Stream<Item = Update> {
    iced::stream::channel(8, async move |mut output: Sender<Update>| {
        let (requests, mut asked) = unbounded_channel::<Request>();
        let _ = output.send(Update::Link(Backend::Network, requests)).await;
        let mut known = None;
        loop {
            let now = network().await;
            if known.as_ref() != Some(&now) {
                known = Some(now.clone());
                let _ = output.send(Update::Network(now)).await;
            }
            // Asked to switch, or time to look again.
            let wait = tokio::time::sleep(Duration::from_secs(5));
            tokio::select! {
                request = asked.recv() => match request {
                    Some(Request::Network(on)) => {
                        let state = if on { "on" } else { "off" };
                        if run("nmcli", &["networking", state]).await.is_none() {
                            warn!("NetworkManager refused to switch the network");
                        }
                        tokio::time::sleep(Duration::from_millis(600)).await;
                    }
                    Some(_) => {}
                    None => return,
                },
                () = wait => {}
            }
        }
    })
}

pub fn bluetooth_subscription() -> Subscription<Update> {
    Subscription::run(bluetooth_service)
}

fn bluetooth_service() -> impl Stream<Item = Update> {
    iced::stream::channel(8, async move |mut output: Sender<Update>| {
        let (requests, mut asked) = unbounded_channel::<Request>();
        let _ = output
            .send(Update::Link(Backend::Bluetooth, requests))
            .await;
        let mut known = None;
        loop {
            let now = bluetooth().await;
            if known.as_ref() != Some(&now) {
                known = Some(now.clone());
                let _ = output.send(Update::Bluetooth(now)).await;
            }
            let wait = tokio::time::sleep(Duration::from_secs(5));
            tokio::select! {
                request = asked.recv() => match request {
                    Some(Request::Bluetooth(on)) => {
                        let state = if on { "on" } else { "off" };
                        if run("bluetoothctl", &["power", state]).await.is_none() {
                            warn!("BlueZ refused to switch Bluetooth");
                        }
                        tokio::time::sleep(Duration::from_millis(600)).await;
                    }
                    Some(_) => {}
                    None => return,
                },
                () = wait => {}
            }
        }
    })
}

pub fn brightness_subscription() -> Subscription<Update> {
    Subscription::run(brightness_service)
}

fn brightness_service() -> impl Stream<Item = Update> {
    iced::stream::channel(8, async move |mut output: Sender<Update>| {
        let (requests, mut asked) = unbounded_channel::<Request>();
        let _ = output
            .send(Update::Link(Backend::Brightness, requests))
            .await;

        // Talking to a screen takes a while: the screens are listed once.
        let detected = run("ddcutil", &["detect", "--brief"]).await;
        let displays = detected.as_deref().map(parse_displays).unwrap_or_default();
        let mut level = None;
        if let Some(first) = displays.first() {
            let answer = run(
                "ddcutil",
                &["getvcp", "10", "--brief", "--display", &first.to_string()],
            )
            .await;
            level = answer.as_deref().and_then(parse_brightness);
        }
        if level.is_none() {
            debug!("no screen answers over DDC/CI (is ddcutil installed?): brightness unavailable");
        }
        let _ = output.send(Update::Brightness(level)).await;

        while let Some(request) = asked.recv().await {
            let Request::Brightness(mut wanted) = request else {
                continue;
            };
            // While the slider moves, only where it ends up matters.
            tokio::time::sleep(Duration::from_millis(250)).await;
            while let Ok(later) = asked.try_recv() {
                if let Request::Brightness(value) = later {
                    wanted = value;
                }
            }
            let value = ((wanted * 100.0).round() as u32).to_string();
            for display in &displays {
                let display = display.to_string();
                run("ddcutil", &["setvcp", "10", &value, "--display", &display]).await;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVICES: &str = "wifi:connected:Freebox-5GHz\nloopback:connected (externally):lo\nbridge:connected (externally):virbr0\nwifi-p2p:disconnected:\nethernet:unavailable:\n";

    #[test]
    fn a_wifi_connection_goes_by_the_name_of_its_network() {
        let network = parse_network("enabled\n", DEVICES);

        assert!(network.on && network.available);
        assert_eq!(network.detail, "Freebox-5GHz");
    }

    #[test]
    fn a_cable_wins_over_wifi() {
        let devices = "wifi:connected:Freebox\nethernet:connected:Connexion filaire 1\n";

        assert_eq!(parse_network("enabled", devices).detail, "Ethernet");
    }

    #[test]
    fn a_network_name_may_hold_colons() {
        let network = parse_network("enabled", "wifi:connected:Chez: nous\n");

        assert_eq!(network.detail, "Chez: nous");
    }

    #[test]
    fn a_network_that_is_on_without_connection_says_so() {
        let network = parse_network("enabled", "ethernet:unavailable:\n");

        assert!(network.on);
        assert_eq!(network.detail, "Déconnecté");
    }

    #[test]
    fn a_network_switched_off_is_off_whatever_the_devices_say() {
        let network = parse_network("disabled\n", DEVICES);

        assert!(!network.on);
        assert_eq!(network.detail, "Désactivé");
    }

    #[test]
    fn bluetooth_names_the_device_connected() {
        let controller = "Controller FC:B0:DE:7E:41:2A (public)\n\tName: tour\n\tPowered: yes\n";
        let devices = "Device 11:22:33:44:55:66 Casque de Jimmy\n";

        let bluetooth = parse_bluetooth(controller, devices);

        assert!(bluetooth.on && bluetooth.available);
        assert_eq!(bluetooth.detail, "Casque de Jimmy");
    }

    #[test]
    fn bluetooth_counts_several_devices_and_admits_having_none() {
        let controller = "Controller FC:B0:DE:7E:41:2A (public)\n\tPowered: yes\n";
        let two = "Device 11:22:33:44:55:66 Casque\nDevice 11:22:33:44:55:77 Manette\n";

        assert_eq!(parse_bluetooth(controller, two).detail, "2 appareils");
        assert_eq!(parse_bluetooth(controller, "").detail, "Activé");
    }

    #[test]
    fn bluetooth_switched_off_is_off() {
        let controller = "Controller FC:B0:DE:7E:41:2A (public)\n\tPowered: no\n";

        let bluetooth = parse_bluetooth(controller, "Device 11:22:33:44:55:66 Casque\n");

        assert!(!bluetooth.on && bluetooth.available);
        assert_eq!(bluetooth.detail, "Désactivé");
    }

    #[test]
    fn without_controller_bluetooth_is_not_available() {
        assert!(!parse_bluetooth("No default controller available\n", "").available);
    }

    #[test]
    fn the_brightness_of_a_screen_is_read_against_its_maximum() {
        assert_eq!(parse_brightness("VCP 10 C 70 100\n"), Some(0.7));
        assert_eq!(parse_brightness("VCP 10 C 128 255"), Some(128.0 / 255.0));
    }

    #[test]
    fn an_answer_that_is_not_a_brightness_gives_none() {
        assert_eq!(parse_brightness("VCP 10 ERR"), None);
        assert_eq!(parse_brightness("Display not found"), None);
        assert_eq!(parse_brightness("VCP 10 C 5 0"), None);
        assert_eq!(parse_brightness(""), None);
    }

    #[test]
    fn the_screens_that_answer_are_numbered() {
        let answer = "Display 1\n   I2C bus:  /dev/i2c-4\n   Monitor:  DEL:AW3418DW\n\nDisplay 2\n   I2C bus:  /dev/i2c-5\n\nInvalid display\n   I2C bus:  /dev/i2c-6\n";

        assert_eq!(parse_displays(answer), [1, 2]);
        assert!(parse_displays("").is_empty());
    }
}
