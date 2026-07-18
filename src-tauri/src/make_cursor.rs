//! Wiimote -> cursor backend for a Mayflash DolphinBar in **mode 4**.
//!
//! In mode 4 the DolphinBar is both the IR sensor bar (its own IR LEDs are what
//! the remote's camera looks at) AND the Bluetooth host, so each synced Wiimote
//! shows up to the PC as a plain USB-HID device under Nintendo's vendor id
//! (0x057E). That means: no Bluetooth code, just HID reads/writes.
//!
//! This module:
//!   * opens player 1's Wiimote,
//!   * runs the (fiddly) IR enable sequence so the camera actually reports dots,
//!   * reads report 0x33 (buttons + accel + 12 IR bytes) in a loop,
//!   * turns the two IR dots into a smoothed { x, y, visible, rotation } cursor,
//!   * emits everything to the frontend via the `wiimote-update` event.
//!
//! Frontend (JS):
//!   import { listen } from "@tauri-apps/api/event";
//!   await listen("wiimote-update", (e) => { /* e.payload = { x, y, visible, rotation, buttons, ... } */ });
//!
//! Lifecycle:
//!   invoke("start_wiimote")  // begin reading (call on app start / after Dolphin closes)
//!   invoke("stop_wiimote")   // release the device BEFORE launching Dolphin, so it can grab the remote
//!
//! Requires `hidapi = "2"` in Cargo.toml, `mod make_cursor;` in lib.rs, and both
//! commands registered in `tauri::generate_handler![]`.

use hidapi::{HidApi, HidDevice};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

// ---------------------------------------------------------------------------
// Tunables — adjust these by feel once it's running.
// ---------------------------------------------------------------------------

/// The camera image is mirrored horizontally relative to where you point, so X
/// almost always needs flipping. Y depends on whether the DolphinBar sits above
/// or below your screen — if vertical feels inverted, flip this.
const FLIP_X: bool = true;
const FLIP_Y: bool = false;

/// Scales motion around the screen centre. 1.0 = raw mapping. Raise it (e.g. 1.3)
/// if you have to move the remote too far to reach the screen edges.
const SENS_X: f32 = 2.0;
const SENS_Y: f32 = 1.0;

/// Low-pass smoothing factor, 0..1. Lower = smoother but laggier. Raw IR is very
/// jittery, so some smoothing is mandatory. ~0.35 is a good starting point.
const SMOOTHING: f32 = 0.35;

/// IR field is 1024x768 (10-bit X 0..1023, Y 0..767), centre at (512, 384).
const IR_W: f32 = 1023.0;
const IR_H: f32 = 767.0;
const IR_CX: f32 = 512.0;
const IR_CY: f32 = 384.0;

const WIIMOTE_VID: u16 = 0x057E;
const WIIMOTE_PIDS: [u16; 2] = [0x0306, 0x0330]; // Wiimote / Wiimote Plus(-TR)

/// IR sensitivity blocks (wiiuse "level 3" — a reliable middle ground).
const IR_SENSITIVITY_BLOCK1: [u8; 9] = [0x02, 0x00, 0x00, 0x71, 0x01, 0x00, 0xAA, 0x00, 0x64];
const IR_SENSITIVITY_BLOCK2: [u8; 2] = [0x63, 0x03];

/// Delay between init writes. The remote needs a moment to process each one.
/// (More robust would be waiting for the 0x22 ack after each write.)
const INIT_DELAY: Duration = Duration::from_millis(50);

// ---------------------------------------------------------------------------
// Shared state so we can cleanly start/stop (and release the device for Dolphin).
// ---------------------------------------------------------------------------

static WIIMOTE_RUNNING: AtomicBool = AtomicBool::new(false);
static WIIMOTE_THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// Payload sent to the frontend.
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone, Default, PartialEq)]
pub struct Buttons {
    pub a: bool,
    pub b: bool,
    pub one: bool,
    pub two: bool,
    pub plus: bool,
    pub minus: bool,
    pub home: bool,
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
}

#[derive(Serialize, Clone)]
pub struct WiimoteUpdate {
    /// Cursor X in 0..1 (multiply by your window width on the frontend).
    pub x: f32,
    /// Cursor Y in 0..1.
    pub y: f32,
    /// True only when the pointer is valid (>= 2 IR dots in view). When false,
    /// x/y/rotation hold their last value — the frontend should freeze or hide.
    pub visible: bool,
    /// Remote roll in degrees, for tilting the cursor "hand" graphic.
    pub rotation: f32,
    /// Decoded button states (valid even when the cursor isn't).
    pub buttons: Buttons,
    /// Raw high 8 bits of each accelerometer axis (the 2 extra LSBs live in the
    /// button bytes if you ever want full 10-bit precision).
    pub accel_x: u8,
    pub accel_y: u8,
    pub accel_z: u8,
}

// ---------------------------------------------------------------------------
// HID output helpers.
// ---------------------------------------------------------------------------

/// Send a Wiimote output report. We always pad to 22 bytes (the remote's output
/// report length) because some Windows HID stacks reject short writes. Bit 0 of
/// the first data byte is the rumble motor — we leave it 0 everywhere so we never
/// accidentally buzz the remote.
fn send(dev: &HidDevice, report_id: u8, data: &[u8]) -> Result<(), String> {
    let mut buf = [0u8; 22];
    buf[0] = report_id;
    buf[1..1 + data.len()].copy_from_slice(data);
    dev.send_output_report(&buf).map_err(|e| e.to_string())?; // was dev.write(&buf)
    Ok(())
}

/// Write to the Wiimote's control registers (output report 0x16). `addr` is the
/// 3-byte big-endian register offset; flag 0x04 selects the control-register space.
fn write_register(dev: &HidDevice, addr: [u8; 3], data: &[u8]) -> Result<(), String> {
    let mut payload = vec![0x04, addr[0], addr[1], addr[2], data.len() as u8];
    payload.extend_from_slice(data);
    send(dev, 0x16, &payload)
}

/// The canonical IR bring-up. Skip or reorder any step and you typically get no
/// dots at all, with no error to tell you why.
fn enable_ir(dev: &HidDevice) -> Result<(), String> {
    // Player 1 LED so the user knows they're connected.
    send(dev, 0x11, &[0x10])?;
    thread::sleep(INIT_DELAY);

    // Enable IR camera (both of these are required; one alone does nothing).
    send(dev, 0x13, &[0x04])?;
    thread::sleep(INIT_DELAY);
    send(dev, 0x1A, &[0x04])?;
    thread::sleep(INIT_DELAY);

    // Put the camera into a known state.
    write_register(dev, [0xB0, 0x00, 0x30], &[0x08])?;
    thread::sleep(INIT_DELAY);

    // Sensitivity.
    write_register(dev, [0xB0, 0x00, 0x00], &IR_SENSITIVITY_BLOCK1)?;
    thread::sleep(INIT_DELAY);
    write_register(dev, [0xB0, 0x00, 0x1A], &IR_SENSITIVITY_BLOCK2)?;
    thread::sleep(INIT_DELAY);

    // IR mode 3 = "extended" (3 bytes/object: x, y, size) — matches report 0x33.
    write_register(dev, [0xB0, 0x00, 0x33], &[0x03])?;
    thread::sleep(INIT_DELAY);

    // Finalize.
    write_register(dev, [0xB0, 0x00, 0x30], &[0x08])?;
    thread::sleep(INIT_DELAY);

    // Reporting mode 0x33 = core buttons + accel + 12 IR bytes, continuous (0x04).
    send(dev, 0x12, &[0x04, 0x33])?;
    thread::sleep(INIT_DELAY);

    Ok(())
}

// ---------------------------------------------------------------------------
// Parsing report 0x33.
// ---------------------------------------------------------------------------

/// One IR blob: position in IR space, brightness "size", and whether it's real.
#[derive(Clone, Copy)]
struct IrDot {
    x: f32,
    y: f32,
    size: u8,
    present: bool,
}

/// Extract the four IR objects from the 12 IR bytes (extended mode).
fn parse_ir(buf: &[u8]) -> [IrDot; 4] {
    let mut dots = [IrDot {
        x: 0.0,
        y: 0.0,
        size: 0,
        present: false,
    }; 4];
    for i in 0..4 {
        let o = 6 + i * 3; // IR bytes start at offset 6 in report 0x33
        let b0 = buf[o] as u16;
        let b1 = buf[o + 1] as u16;
        let b2 = buf[o + 2] as u16;

        let x = b0 | ((b2 & 0x30) << 4); // X[9:8] live in b2 bits 4-5
        let y = b1 | ((b2 & 0xC0) << 2); // Y[9:8] live in b2 bits 6-7
        let size = (b2 & 0x0F) as u8;

        // No object -> all bytes 0xFF -> x == y == 1023.
        let present = !(x == 1023 && y == 1023);
        dots[i] = IrDot {
            x: x as f32,
            y: y as f32,
            size,
            present,
        };
    }
    dots
}

fn parse_buttons(buf: &[u8]) -> Buttons {
    let b1 = buf[1];
    let b2 = buf[2];
    Buttons {
        left: b1 & 0x01 != 0,
        right: b1 & 0x02 != 0,
        down: b1 & 0x04 != 0,
        up: b1 & 0x08 != 0,
        plus: b1 & 0x10 != 0,
        two: b2 & 0x01 != 0,
        one: b2 & 0x02 != 0,
        b: b2 & 0x04 != 0,
        a: b2 & 0x08 != 0,
        minus: b2 & 0x10 != 0,
        home: b2 & 0x80 != 0,
    }
}

// ---------------------------------------------------------------------------
// IR dots -> cursor.
// ---------------------------------------------------------------------------

fn compute_cursor(dots: &[IrDot; 4], t: &mut IrTracker) -> Option<(f32, f32, f32)> {
    let mut present: Vec<&IrDot> = dots.iter().filter(|d| d.present).collect();

    if present.len() >= 2 {
        present.sort_by(|a, b| b.size.cmp(&a.size));
        let (mut a, mut b) = (present[0], present[1]);
        if a.x > b.x {
            std::mem::swap(&mut a, &mut b);
        } // a = left, b = right

        let mx = (a.x + b.x) / 2.0;
        let my = (a.y + b.y) / 2.0;

        // remember the geometry so we can survive a one-dot frame
        t.hx = b.x - mx;
        t.hy = b.y - my;
        t.theta = (b.y - a.y).atan2(b.x - a.x);
        t.last_mx = mx;
        t.last_my = my;
        t.have = true;

        return Some(map_midpoint(mx, my, t.theta));
    }

    if present.len() == 1 && t.have {
        // one dot in view: rebuild the midpoint from the remembered offset.
        let d = present[0];
        let (mx, my) = if d.x < t.last_mx {
            (d.x + t.hx, d.y + t.hy) // this is the left dot
        } else {
            (d.x - t.hx, d.y - t.hy) // this is the right dot
        };
        t.last_mx = mx;
        t.last_my = my;
        return Some(map_midpoint(mx, my, t.theta));
    }

    None
}

/// Turn the IR dots into a normalised cursor + roll. Returns None when fewer than
/// two dots are visible (pointer is undefined). Output: (x 0..1, y 0..1, deg).
// fn compute_cursor(dots: &[IrDot; 4]) -> Option<(f32, f32, f32)> {
//     // Keep only real dots, take the two brightest (guards against reflections).
//     let mut present: Vec<&IrDot> = dots.iter().filter(|d| d.present).collect();
//     if present.len() < 2 {
//         return None;
//     }
//     present.sort_by(|a, b| b.size.cmp(&a.size));

//     // Order left -> right so the roll angle has a stable sign.
//     let (mut a, mut b) = (present[0], present[1]);
//     if a.x > b.x {
//         std::mem::swap(&mut a, &mut b);
//     }

//     // Roll = angle of the line between the two dots.
//     let theta = (b.y - a.y).atan2(b.x - a.x);

//     // Midpoint = where you're pointing (before roll correction).
//     let mx = (a.x + b.x) / 2.0;
//     let my = (a.y + b.y) / 2.0;

//     // Roll-compensate: rotate the midpoint about the IR centre by -theta, so
//     // twisting the remote doesn't drag the cursor diagonally.
//     let dx = mx - IR_CX;
//     let dy = my - IR_CY;
//     let (s, c) = theta.sin_cos();
//     let rx = dx * c + dy * s + IR_CX;
//     let ry = -dx * s + dy * c + IR_CY;

//     // Normalise, flip, scale.
//     let mut nx = rx / IR_W;
//     let mut ny = ry / IR_H;
//     if FLIP_X {
//         nx = 1.0 - nx;
//     }
//     if FLIP_Y {
//         ny = 1.0 - ny;
//     }
//     nx = ((nx - 0.5) * SENSITIVITY + 0.5).clamp(0.0, 1.0);
//     ny = ((ny - 0.5) * SENSITIVITY + 0.5).clamp(0.0, 1.0);

//     Some((nx, ny, theta.to_degrees()))
// }

// ---------------------------------------------------------------------------
// The read loop.
// ---------------------------------------------------------------------------

fn open_wiimote(api: &HidApi) -> Result<HidDevice, String> {
    let mut found = 0;
    for info in api.device_list() {
        if info.vendor_id() != WIIMOTE_VID || !WIIMOTE_PIDS.contains(&info.product_id()) {
            continue;
        }
        found += 1;
        log::info!(
            "Wiimote candidate: pid={:#06x} interface={} usage_page={:#x} product={:?}",
            info.product_id(),
            info.interface_number(),
            info.usage_page(),
            info.product_string()
        );

        let dev = match info.open_device(api) {
            Ok(d) => d,
            Err(_) => continue,
        };

        // Probe: set the player-1 LED. A live slot accepts it; an empty slot won't.
        let mut probe = [0u8; 22];
        probe[0] = 0x11;
        probe[1] = 0x10;
        if dev.send_output_report(&probe).is_ok() {
            log::info!("-> selected this interface (write accepted)");
            return Ok(dev);
        }
        log::info!("-> probe write rejected, trying next interface");
    }
    Err(format!(
        "Found {found} Wiimote interface(s), none accepted a write. Is a remote actually synced right now (a player LED lit), and is the bar on mode 4?"
    ))
}

fn sleep_unless_stopped(ms: u64) {
    let mut left = ms;
    while left > 0 && WIIMOTE_RUNNING.load(Ordering::SeqCst) {
        let chunk = left.min(100);
        thread::sleep(Duration::from_millis(chunk));
        left -= chunk;
    }
}

struct IrTracker {
    hx: f32,      // half the gap between the dots (right dot minus midpoint), x
    hy: f32,      // ... y
    theta: f32,   // last roll angle (radians)
    last_mx: f32, // last midpoint in IR space, to tell which dot a lone point is
    last_my: f32,
    have: bool,
}

impl IrTracker {
    fn new() -> Self {
        IrTracker {
            hx: 0.0,
            hy: 0.0,
            theta: 0.0,
            last_mx: IR_CX,
            last_my: IR_CY,
            have: false,
        }
    }
}

/// Roll-compensate, normalise, flip and scale a midpoint -> (nx, ny, degrees).
fn map_midpoint(mx: f32, my: f32, theta: f32) -> (f32, f32, f32) {
    let dx = mx - IR_CX;
    let dy = my - IR_CY;
    let (s, c) = theta.sin_cos();
    let rx = dx * c + dy * s + IR_CX;
    let ry = -dx * s + dy * c + IR_CY;
    let mut nx = rx / IR_W;
    let mut ny = ry / IR_H;
    if FLIP_X {
        nx = 1.0 - nx;
    }
    if FLIP_Y {
        ny = 1.0 - ny;
    }
    nx = ((nx - 0.5) * SENS_X + 0.5).clamp(0.0, 1.0);
    ny = ((ny - 0.5) * SENS_Y + 0.5).clamp(0.0, 1.0);
    (nx, ny, theta.to_degrees())
}

const DISCONNECT_TIMEOUT: Duration = Duration::from_millis(1500);

/// Idle heartbeat for `wiimote-update` events. Reports arrive at HID rate,
/// but an unchanged cursor is only re-sent this often; meaningful changes
/// (movement, visibility, rotation, buttons) still emit immediately so
/// pointing stays smooth.
const EMIT_HEARTBEAT: Duration = Duration::from_millis(500);
const EMIT_POS_EPSILON: f32 = 0.002;
const EMIT_ROT_EPSILON: f32 = 0.5;

fn wiimote_supervisor(app: &AppHandle, player: u8) {
    let mut api = match HidApi::new() {
        Ok(a) => a,
        Err(e) => {
            log::error!("hidapi init failed: {e}");
            return;
        }
    };

    let mut announced_wait = false;

    while WIIMOTE_RUNNING.load(Ordering::SeqCst) {
        let _ = api.refresh_devices(); // pick up remotes synced after startup

        let dev = match open_wiimote(&api) {
            Ok(d) => d,
            Err(_) => {
                if !announced_wait {
                    log::info!("Waiting for a controller...");
                    announced_wait = true;
                }
                sleep_unless_stopped(1000);
                continue;
            }
        };
        announced_wait = false;

        if let Err(e) = enable_ir(&dev) {
            log::error!("IR init failed, will retry: {e}");
            sleep_unless_stopped(1000);
            continue;
        }

        run_read_loop(app, player, &dev);
        // dev drops here -> loop re-scans and reconnects
    }
}

fn emit_button_edges(
    app: &AppHandle,
    player: u8,
    prev: &Buttons,
    cur: &Buttons,
    x: f32,
    y: f32,
    visible: bool,
) {
    macro_rules! edge {
        ($field:ident, $name:expr) => {
            if cur.$field != prev.$field {
                let _ = app.emit("wiimote-button", serde_json::json!({
                    "player": player,
                    "button": $name,
                    "pressed": cur.$field,
                    "x": x, "y": y, "visible": visible,
                }));
            }
        };
    }
    edge!(a, "a");
    edge!(b, "b");
    edge!(one, "one");
    edge!(two, "two");
    edge!(plus, "plus");
    edge!(minus, "minus");
    edge!(home, "home");
    edge!(up, "up");
    edge!(down, "down");
    edge!(left, "left");
    edge!(right, "right");
}

/// Reads one connected Wiimote until it disconnects or we're told to stop.
fn run_read_loop(app: &AppHandle, player: u8, dev: &HidDevice) {
    let _ = app.emit("wiimote-connected", player);

    let mut sx = 0.5f32;
    let mut sy = 0.5f32;
    let mut srot = 0.0f32;
    let mut acquired = false;
    let mut tracker = IrTracker::new(); // drop this line if you didn't add single-dot tracking
    let mut last_data = Instant::now();
    let mut buf = [0u8; 64];

    let mut prev_buttons = Buttons::default();

    // throttling state: last emitted values + when
    let mut last_emit = Instant::now() - EMIT_HEARTBEAT;
    let mut last_sent: Option<(f32, f32, f32, bool)> = None;

    while WIIMOTE_RUNNING.load(Ordering::SeqCst) {
        match dev.read_timeout(&mut buf, 100) {
            Ok(0) => {
                if last_data.elapsed() > DISCONNECT_TIMEOUT {
                    let _ = app.emit("wiimote-disconnected", player);
                    return; // hand back to the supervisor to reconnect
                }
            }
            Ok(n) => {
                if n < 18 || buf[0] != 0x33 {
                    continue;
                }
                last_data = Instant::now();

                let buttons = parse_buttons(&buf);
                let accel_x = buf[3];
                let accel_y = buf[4];
                let accel_z = buf[5];
                let dots = parse_ir(&buf);

                let visible = match compute_cursor(&dots, &mut tracker) {
                    Some((nx, ny, rot)) => {
                        if !acquired {
                            sx = nx;
                            sy = ny;
                            srot = rot;
                            acquired = true;
                        } else {
                            sx += SMOOTHING * (nx - sx);
                            sy += SMOOTHING * (ny - sy);
                            srot += SMOOTHING * (rot - srot);
                        }
                        true
                    }
                    None => {
                        acquired = false;
                        false
                    }
                };

                let buttons_changed = buttons != prev_buttons;
                emit_button_edges(app, player, &prev_buttons, &buttons, sx, sy, visible);
                prev_buttons = buttons.clone();

                // emit immediately on meaningful change, else 500ms heartbeat
                // (accel deliberately excluded: it jitters on every report)
                let changed = buttons_changed
                    || match last_sent {
                        None => true,
                        Some((px, py, pr, pv)) => {
                            pv != visible
                                || (sx - px).abs() > EMIT_POS_EPSILON
                                || (sy - py).abs() > EMIT_POS_EPSILON
                                || (srot - pr).abs() > EMIT_ROT_EPSILON
                        }
                    };

                if changed || last_emit.elapsed() >= EMIT_HEARTBEAT {
                    last_emit = Instant::now();
                    last_sent = Some((sx, sy, srot, visible));

                    let update = WiimoteUpdate {
                        x: sx,
                        y: sy,
                        visible,
                        rotation: srot,
                        buttons,
                        accel_x,
                        accel_y,
                        accel_z,
                    };
                    let _ = app.emit("wiimote-update", &update);
                }
            }
            Err(_) => {
                let _ = app.emit("wiimote-disconnected", player);
                return; // device gone -> supervisor reconnects
            }
        }
    }
    // RUNNING == false: intentional stop (Dolphin handoff) — stay quiet
}

// ---------------------------------------------------------------------------
// Tauri commands.
// ---------------------------------------------------------------------------

/// Start reading the Wiimote and emitting `wiimote-update` events.
/// Idempotent: calling it while already running is a no-op.
#[tauri::command]
pub fn start_wiimote(app: AppHandle) -> Result<(), String> {
    // Claim the "running" flag; if it was already true, we're already going.
    if WIIMOTE_RUNNING.swap(true, Ordering::SeqCst) {
        return Ok(());
    }

    let app_for_thread = app.clone();
    let handle = thread::spawn(move || {
        wiimote_supervisor(&app_for_thread, 1);
        WIIMOTE_RUNNING.store(false, Ordering::SeqCst);
    });

    *WIIMOTE_THREAD.lock().unwrap() = Some(handle);
    Ok(())
}

/// Stop reading and **release the device**. This blocks until the read thread has
/// fully exited, so when it returns you're guaranteed the HID handle is closed —
/// call this right before launching Dolphin so it can take the remote.
#[tauri::command]
pub fn stop_wiimote() {
    WIIMOTE_RUNNING.store(false, Ordering::SeqCst);
    if let Some(handle) = WIIMOTE_THREAD.lock().unwrap().take() {
        let _ = handle.join();
    }
}
