use anyhow::{Context, Result};
use futures_lite::{future, StreamExt};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const PORTAL_DESTINATION: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const SCREENSHOT_INTERFACE: &str = "org.freedesktop.portal.Screenshot";
const REQUEST_INTERFACE: &str = "org.freedesktop.portal.Request";

pub(crate) struct ScreenColorPickerWorker {
    pub(crate) cancel_tx: async_channel::Sender<()>,
    pub(crate) rx: mpsc::Receiver<Result<Option<[u8; 3]>, String>>,
}

pub(crate) fn spawn(repaint: egui::Context) -> ScreenColorPickerWorker {
    let (result_tx, rx) = mpsc::channel();
    let (cancel_tx, cancel_rx) = async_channel::bounded(1);
    std::thread::Builder::new()
        .name("entropy-screen-color-picker".to_owned())
        .spawn(move || {
            let result =
                future::block_on(pick_color(cancel_rx)).map_err(|error| format!("{error:#}"));
            let _ = result_tx.send(result);
            repaint.request_repaint();
        })
        .expect("failed to spawn screen color picker worker");
    ScreenColorPickerWorker { cancel_tx, rx }
}

async fn pick_color(cancel_rx: async_channel::Receiver<()>) -> Result<Option<[u8; 3]>> {
    static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

    let connection = zbus::Connection::session()
        .await
        .context("Failed to connect to the desktop portal")?;
    let portal = zbus::Proxy::new(
        &connection,
        PORTAL_DESTINATION,
        PORTAL_PATH,
        SCREENSHOT_INTERFACE,
    )
    .await
    .context("Failed to create the screenshot portal proxy")?;
    let unique_name = connection
        .unique_name()
        .context("Desktop portal connection has no unique D-Bus name")?
        .as_str()
        .trim_start_matches(':')
        .replace('.', "_");
    let token = format!(
        "entropy_color_{}",
        REQUEST_ID.fetch_add(1, Ordering::Relaxed)
    );
    let expected_path = format!("/org/freedesktop/portal/desktop/request/{unique_name}/{token}");
    let request = zbus::Proxy::new(
        &connection,
        PORTAL_DESTINATION,
        expected_path.as_str(),
        REQUEST_INTERFACE,
    )
    .await
    .context("Failed to create the color picker request proxy")?;
    let mut responses = request
        .receive_signal("Response")
        .await
        .context("Failed to listen for the color picker response")?;

    let mut options = HashMap::<&str, Value<'_>>::new();
    options.insert("handle_token", Value::from(token.as_str()));
    let returned_path: OwnedObjectPath = portal
        .call("PickColor", &("", options))
        .await
        .context("Desktop portal PickColor failed")?;
    if returned_path.as_str() != expected_path {
        log::debug!(
            "Desktop portal returned color picker path {}, expected {}",
            returned_path,
            expected_path
        );
    }

    enum PickerEvent {
        Response(Option<zbus::Message>),
        Cancel,
    }
    let event = future::race(
        async { PickerEvent::Response(responses.next().await) },
        async {
            let _ = cancel_rx.recv().await;
            PickerEvent::Cancel
        },
    )
    .await;

    match event {
        PickerEvent::Cancel => {
            request
                .call::<_, _, ()>("Close", &())
                .await
                .context("Failed to close the color picker request")?;
            Ok(None)
        }
        PickerEvent::Response(Some(message)) => {
            let (response, mut results): (u32, HashMap<String, OwnedValue>) = message
                .body()
                .deserialize()
                .context("Malformed color picker response")?;
            match response {
                0 => {
                    let color = results
                        .remove("color")
                        .context("Color picker response did not contain a color")?;
                    let (red, green, blue): (f64, f64, f64) = color
                        .try_into()
                        .context("Color picker returned an invalid RGB tuple")?;
                    Ok(Some([
                        normalized_component(red),
                        normalized_component(green),
                        normalized_component(blue),
                    ]))
                }
                1 => Ok(None),
                code => anyhow::bail!("Color picker request failed with response code {code}"),
            }
        }
        PickerEvent::Response(None) => anyhow::bail!("Color picker response stream stopped"),
    }
}

pub(crate) fn normalized_component(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_rgb_tuple_round_trips_through_owned_value() {
        let owned = OwnedValue::try_from(Value::from((0.1_f64, 0.5_f64, 1.0_f64))).unwrap();
        let rgb: (f64, f64, f64) = owned.try_into().unwrap();

        assert_eq!(rgb, (0.1, 0.5, 1.0));
    }

    #[test]
    fn portal_color_components_are_clamped_and_rounded() {
        assert_eq!(normalized_component(-0.1), 0);
        assert_eq!(normalized_component(0.5), 128);
        assert_eq!(normalized_component(1.1), 255);
    }
}
