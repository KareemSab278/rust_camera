use chrono::Local;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Mutex,
    mpsc::{self},
};
use std::thread;
use std::time::Duration;

/*
    im gonna kiss here.
    tauri should send the camera identifier to here but for now ill just search for the camera and use it. if no camera found then return error to frontend.
    find all cameras - should be only 1 plugged in!
    basically just record with the plugged in camera else return Err 'failed to init cam'
    then stop recording after timeout or if stop_recording called.
    control with 1 atomic bool. check if recording active or not in cam recording loop.
    stop_recording fn switches the atomic bool and hands the video to ffmpeg to save to disk
    ffmpeg runs on separate thread and is killed when ffmpeg process completes.
    atomic bool then switched back to normal state and ready for next recording.

    this ensure the camera is always ready for another video but the ffmpeg process stays running on separate threads.
    ffmpeg is a fire and forget process that will save the video to disk and then exit. we dont need to wait for it to finish.
    we must rely on ffmpeg to work correctly and save video to disk. if ffmpeg fails then im a shitty dev.
    im not worried about storage as well - 120 gb sd is enough memory since video will be sent and then deleted on success
*/

const MAX_RECORDING_TIMEOUT_SECONDS: u8 = 10;
const RECORDINGS_DIR: &str = "Videos/Recordings";

static RECORDING_STOP: Mutex<Option<mpsc::Sender<()>>> = Mutex::new(None);

pub fn start_recording(camera: &str, timeout: u8) -> Result<PathBuf, io::Error> {
    let duration = clamp_timeout(timeout);

    fs::create_dir_all(RECORDINGS_DIR)?;

    stop_recording()?;

    let timestamp = Local::now().format("%d-%m-%Y-%H-%M-%S");
    let filename = format!("{}-camera.mp4", timestamp);
    let output = Path::new(RECORDINGS_DIR).join(filename);

    let process = start_ffmpeg(camera, &output)?;

    let (stop_sender, stop_receiver) = mpsc::channel();

    {
        let mut recording = RECORDING_STOP.lock().unwrap();
        *recording = Some(stop_sender);
    }

    thread::spawn(move || {
        run_recording(process, duration, stop_receiver);

        let mut recording = RECORDING_STOP.lock().unwrap();
        recording.take();
    });

    Ok(output)
}

#[allow(dead_code)]
pub fn is_recording() -> bool {
    let recording = RECORDING_STOP.lock().unwrap();
    recording.is_some()
}

fn clamp_timeout(timeout: u8) -> u8 {
    if timeout == 0 || timeout > MAX_RECORDING_TIMEOUT_SECONDS {
        MAX_RECORDING_TIMEOUT_SECONDS
    } else {
        timeout
    }
}

fn run_recording(mut process: Child, timeout: u8, stop_receiver: mpsc::Receiver<()>) {
    match stop_receiver.recv_timeout(Duration::from_secs(timeout as u64)) {
        Ok(()) => {
            let _ = stop_ffmpeg(&mut process);
        }

        Err(mpsc::RecvTimeoutError::Timeout) => {
            let _ = stop_ffmpeg(&mut process);
        }

        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let _ = stop_ffmpeg(&mut process);
        }
    }

    match process.wait() {
        Ok(status) if status.success() => {
            println!("FFmpeg finished successfully");
        }

        Ok(status) => {
            eprintln!("FFmpeg failed with status: {}", status);
        }

        Err(e) => {
            eprintln!("Failed waiting for FFmpeg: {}", e);
        }
    }
}

fn stop_ffmpeg(process: &mut Child) -> Result<(), io::Error> {
    if let Some(mut stdin) = process.stdin.take() {
        stdin.write_all(b"q\n")?;
        stdin.flush()?;
    }

    Ok(())
}

fn start_ffmpeg(device: &str, output: &Path) -> Result<Child, io::Error> {
    Command::new("ffmpeg")
        .args(generate_ffmpeg_command(device, output))
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to start FFmpeg: {}", e),
            )
        })
}

pub fn stop_recording() -> Result<(), io::Error> {
    let stop_sender = {
        let mut recording = RECORDING_STOP.lock().unwrap();
        recording.take()
    };

    if let Some(sender) = stop_sender {
        let _ = sender.send(());
    }

    Ok(())
}

#[allow(dead_code)]
pub fn delete_video(video_name: &str) -> Result<(), io::Error> {
    let path = Path::new(RECORDINGS_DIR).join(video_name);

    if !path.exists() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "Video not found"));
    }

    fs::remove_file(path)?;

    Ok(())
}

pub fn list_recordings() -> Result<Vec<String>, io::Error> {
    let entries = match fs::read_dir(RECORDINGS_DIR) {
        Ok(entries) => entries,
        Err(_) => {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to read recordings directory",
            ));
        }
    };

    let recordings = entries
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.ends_with(".mp4"))
        .collect();

    Ok(recordings)
}

/*
    MJPEG camera -> H.264 hardware encoder -> MP4
    this is the most efficient processing for the camera to record and save to disk
    the camera outputs MJPEG frams and also at 30fps and also 1280x720 and 640x480 res
    ffmpeg will do less processing if we keep it the same
*/
fn generate_ffmpeg_command(device: &str, output: &Path) -> Vec<String> {
    vec![
        "-f".into(),
        "v4l2".into(),
        "-input_format".into(),
        "mjpeg".into(),
        "-framerate".into(),
        "30".into(),
        "-video_size".into(),
        "640x480".into(),
        "-i".into(),
        device.into(),
        "-c:v".into(),
        "h264_v4l2m2m".into(),
        "-b:v".into(),
        "2M".into(),
        "-an".into(),
        "-y".into(),
        output.to_string_lossy().into_owned(),
    ]
}

pub fn list_cameras() -> Result<Vec<String>, io::Error> {
    let mut cameras = Vec::new();

    for i in 0..10 {
        let device = format!("/dev/video{}", i);

        if !Path::new(&device).exists() {
            continue;
        }

        let output = Command::new("v4l2-ctl")
            .args(["-d", &device, "--all"])
            .output();

        let output = match output {
            Ok(output) if output.status.success() => output,
            _ => continue,
        };

        let text = String::from_utf8_lossy(&output.stdout);

        if text.contains("Driver name      : uvcvideo")
            && text.contains("Capabilities     : timeperframe")
        {
            cameras.push(device);
        }
    }

    Ok(cameras)
}
