#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _}; // For Base64 encoding
use chrono::Utc;
use image::{codecs::png::PngEncoder, ImageBuffer, Rgba}; // Added for PNG encoding, ImageBuffer, Rgba
use image::ImageEncoder; // Added for PNG encoding
use rand::Rng;
use xcap::{Monitor, Window}; // Replaced screenshots::Screen with xcap types
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use std::fs; // Added for file system operations
use std::io::Cursor; // Added for writing PNG to buffer
use std::path::PathBuf; // Added for path manipulation
use std::sync::{atomic::{AtomicBool, Ordering}, Arc}; // Added AtomicBool and Ordering
use std::time::Duration; // Removed SystemTime import
use tauri::async_runtime::Mutex;
use tauri::{AppHandle, Emitter, Manager, State}; // Added Manager back
use tokio::sync::mpsc::{self, Sender};
use tokio::time::sleep;
use uuid::Uuid;

mod activity_monitor; // Declare the new module
use crate::activity_monitor::{ActivityCounters, ActivityData, listen as activity_listen, get_current_counts}; // Import items

// Represents the possible states of the timer/screenshot task
#[derive(Clone, serde::Serialize, Debug, PartialEq)]
enum TimerCommand {
    Pause,
    Resume,
    Stop,
}

// Represents the current status of the timer
#[derive(Clone, serde::Serialize, Debug, PartialEq)]
enum TimerStatus {
    Stopped,
    Running,
    Paused,
}

// The application state shared across Tauri commands
struct AppState {
    db_pool: Pool<Postgres>,
    timer_status: Arc<Mutex<TimerStatus>>,
    // Channel to send commands (Pause, Resume, Stop) to the running timer task
    command_tx: Arc<Mutex<Option<Sender<TimerCommand>>>>,
    current_session_id: Arc<Mutex<Option<Uuid>>>, // Added
    session_start_time: Arc<Mutex<Option<chrono::DateTime<Utc>>>>, // Added to track start time for elapsed calculation
    activity_counters: Arc<ActivityCounters>, // Added for activity monitoring
    is_session_active: Arc<AtomicBool>, // Flag to control activity counting
}


// Function to capture a screenshot, gather system info, and save everything
async fn capture_and_save(
    db_pool: &Pool<Postgres>,
    session_id: Uuid,
    app_handle: &AppHandle, // Added for emitting event
) -> Result<(), String> {
    // --- Gather System Info using xcap ---
    let monitors = Monitor::all().map_err(|e| format!("Failed to get monitors: {}", e))?;
    let monitor_count = monitors.len() as i32; // Cast usize to i32 for DB

    let windows = Window::all().map_err(|e| format!("Failed to get windows: {}", e))?;
    let open_windows: Vec<String> = windows
        .iter()
        .filter_map(|w| {
            // If the window is minimized, filter it out.
            if w.is_minimized() {
                return None;
            }
            // w.title() returns &str directly
            let title_str = w.title();
            if !title_str.is_empty() {
                Some(title_str.to_string()) // Convert non-empty &str to String
            } else {
                None // Title is empty
            }
        })
        .collect();
    // --- End Gather System Info ---

    // Capture the primary monitor (or the first one found)
    if let Some(monitor) = monitors.first() {
        println!("Capturing monitor: {}", monitor.name());
        let image: ImageBuffer<Rgba<u8>, Vec<u8>> = monitor // xcap returns ImageBuffer
            .capture_image()
            .map_err(|e| format!("Failed to capture screen using xcap: {}", e))?;

        // Encode as PNG
        let mut png_buffer = Cursor::new(Vec::new());
        let encoder = PngEncoder::new(&mut png_buffer);
        encoder
            .write_image(
                image.as_raw(), // Use as_raw() for the underlying buffer
                image.width(),
                image.height(),
                image::ColorType::Rgba8.into() // Ensure .into() is present
            )
            .map_err(|e| format!("Failed to encode PNG: {}", e))?;
        let buffer_data = png_buffer.into_inner(); // Get the Vec<u8>

        let capture_time = Utc::now();
        let screenshot_id = Uuid::new_v4();

        // Insert into DB including new fields, using query() function (runtime check)
        sqlx::query( // Use query()
            r#"
            INSERT INTO screenshots (id, session_id, capture_time, image_data, monitor_count, open_windows)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#
        )
        .bind(screenshot_id)
        .bind(session_id)
        .bind(capture_time)
        .bind(&buffer_data) // BYTEA
        .bind(monitor_count) // INTEGER
        .bind(&open_windows) // TEXT[]
        .execute(db_pool)
        .await
        .map_err(|e| format!("Failed to insert screenshot into DB: {}", e))?;

        println!(
            "Screenshot saved to DB successfully with ID: {} for session: {} (Monitors: {}, Windows: {})",
            screenshot_id, session_id, monitor_count, open_windows.len()
        );

        // --- Save screenshot locally ---
        let screenshots_dir = PathBuf::from("src-tauri/screenshots");
        fs::create_dir_all(&screenshots_dir)
            .map_err(|e| format!("Failed to create screenshots directory: {}", e))?;

        let filename = format!("{}.png", screenshot_id);
        let file_path = screenshots_dir.join(&filename); // Use reference to filename

        fs::write(&file_path, &buffer_data)
            .map_err(|e| format!("Failed to save screenshot file locally: {}", e))?;

        println!("Screenshot saved locally to: {:?}", file_path);
        // --- End save screenshot locally ---


        // Emit event to frontend with the screenshot ID
        app_handle
            .emit("new_screenshot", screenshot_id.to_string()) // Send the ID as string
            .map_err(|e| format!("Failed to emit new_screenshot event: {}", e))?;

        Ok(())
    } else {
        Err("No screens found to capture.".to_string())
    }
}

// The main async task for the timer and screenshot logic
async fn timer_task(
    db_pool: Pool<Postgres>,
    timer_status: Arc<Mutex<TimerStatus>>,
    mut command_rx: mpsc::Receiver<TimerCommand>,
    app_handle: AppHandle,
    session_id: Uuid, // Added
) {
    println!("Timer task started for session {}.", session_id);
    let mut is_paused = false;

    loop {
        // Check for commands (Pause, Resume, Stop) without blocking indefinitely
        match command_rx.try_recv() {
            Ok(TimerCommand::Pause) => {
                println!("Timer task received PAUSE command.");
                is_paused = true;
                *timer_status.lock().await = TimerStatus::Paused;
                // Notify frontend about the status change
                app_handle.emit("timer_status_update", TimerStatus::Paused).unwrap();
            }
            Ok(TimerCommand::Resume) => {
                println!("Timer task received RESUME command.");
                is_paused = false;
                *timer_status.lock().await = TimerStatus::Running;
                 // Notify frontend about the status change
                app_handle.emit("timer_status_update", TimerStatus::Running).unwrap();
            }
            Ok(TimerCommand::Stop) => {
                println!("Timer task received STOP command.");
                *timer_status.lock().await = TimerStatus::Stopped;
                 // Notify frontend about the status change
                app_handle.emit("timer_status_update", TimerStatus::Stopped).unwrap();
                break; // Exit the loop
            }
            Err(mpsc::error::TryRecvError::Empty) => {
                // No command received, continue normal operation
            }
            Err(mpsc::error::TryRecvError::Disconnected) => {
                println!("Timer command channel disconnected. Stopping task.");
                *timer_status.lock().await = TimerStatus::Stopped;
                 // Notify frontend about the status change
                app_handle.emit("timer_status_update", TimerStatus::Stopped).unwrap();
                break; // Exit the loop
            }
        }

        if !is_paused {
            // Generate random delay between 4 and 10 seconds
            let delay_secs = rand::thread_rng().gen_range(4..=10);
            println!("Next screenshot in {} seconds...", delay_secs);
            sleep(Duration::from_secs(delay_secs)).await;

             // Check again for commands received *during* sleep
            match command_rx.try_recv() {
                 Ok(TimerCommand::Pause) => { is_paused = true; *timer_status.lock().await = TimerStatus::Paused; app_handle.emit("timer_status_update", TimerStatus::Paused).unwrap(); continue; }
                 Ok(TimerCommand::Resume) => { is_paused = false; *timer_status.lock().await = TimerStatus::Running; app_handle.emit("timer_status_update", TimerStatus::Running).unwrap(); /* Continue below */ }
                 Ok(TimerCommand::Stop) => { *timer_status.lock().await = TimerStatus::Stopped; app_handle.emit("timer_status_update", TimerStatus::Stopped).unwrap(); break; }
                 Err(_) => { /* Continue below */ }
            }

            if !is_paused { // Check pause status *again* after sleep and potential command
                println!("Taking screenshot for session {}...", session_id);
                // Pass session_id and app_handle to capture_and_save
                if let Err(e) = capture_and_save(&db_pool, session_id, &app_handle).await {
                    eprintln!("Error capturing/saving screenshot: {}", e);
                    app_handle.emit("screenshot_error", e).unwrap_or_else(|err| eprintln!("Failed to emit error: {}", err));
                }
            }
        } else {
            // If paused, sleep for a short duration to avoid busy-waiting
            sleep(Duration::from_millis(500)).await;
        }
    }

    println!("Timer task finished.");
}

// Tauri command to start the timer
#[tauri::command]
async fn start_timer(state: State<'_, AppState>, app_handle: AppHandle) -> Result<(), String> {
    let mut status = state.timer_status.lock().await;
    if *status != TimerStatus::Stopped {
        return Err("Timer is already running or paused.".to_string());
    }

    println!("Starting timer...");

    // --- Reset Activity Counters and Activate Listening ---
    state.activity_counters.key_presses.store(0, Ordering::Relaxed);
    state.activity_counters.mouse_clicks.store(0, Ordering::Relaxed);
    state.is_session_active.store(true, Ordering::Relaxed); // Enable counting
    println!("Activity counters reset and listening activated.");
    // --- End Reset ---

    *status = TimerStatus::Running;

    // --- Session Handling ---
    let session_id = Uuid::new_v4();
    let start_time = Utc::now();
    *state.current_session_id.lock().await = Some(session_id);
    *state.session_start_time.lock().await = Some(start_time); // Store start time

    // Insert new session into DB
    // Use query() function
    sqlx::query("INSERT INTO sessions (id, start_time) VALUES ($1, $2)") // Use query()
        .bind(session_id)
        .bind(start_time)
        .execute(&state.db_pool)
        .await
        .map_err(|e| format!("Failed to insert session into DB: {}", e))?;
    println!("Started session with ID: {}", session_id);
    // --- End Session Handling ---


    let db_pool = state.db_pool.clone();
    let status_clone = Arc::clone(&state.timer_status);
    let (tx, rx) = mpsc::channel(1);

    *state.command_tx.lock().await = Some(tx);

    // Spawn the timer task with session_id
    tokio::spawn(timer_task(
        db_pool,
        status_clone,
        rx,
        app_handle.clone(),
        session_id, // Pass session_id
    ));

    app_handle.emit("timer_status_update", TimerStatus::Running).unwrap();
    Ok(())
}

// Tauri command to stop the timer
#[tauri::command]
async fn stop_timer(state: State<'_, AppState>, app_handle: AppHandle) -> Result<(), String> {
    let mut status = state.timer_status.lock().await;
     if *status == TimerStatus::Stopped {
         return Err("Timer is already stopped.".to_string());
     }
     println!("Stopping timer...");

     // --- Stop Activity Counting and Save Counts ---
     state.is_session_active.store(false, Ordering::Relaxed); // Disable counting FIRST
     println!("Activity listening deactivated.");

     let final_key_presses = state.activity_counters.key_presses.load(Ordering::Relaxed) as i32; // Cast to i32 for DB
     let final_mouse_clicks = state.activity_counters.mouse_clicks.load(Ordering::Relaxed) as i32; // Cast to i32 for DB
     println!("Final counts - Keys: {}, Clicks: {}", final_key_presses, final_mouse_clicks);

     // --- Session Handling (Update DB with counts) ---
     let session_id_opt = *state.current_session_id.lock().await;
     if let Some(session_id) = session_id_opt {
         let end_time = Utc::now();
         // Update session end time AND activity counts in DB
         sqlx::query(
             r#"
             UPDATE sessions
             SET end_time = $1, key_press_count = $2, mouse_click_count = $3
             WHERE id = $4
             "#
         )
         .bind(end_time)
         .bind(final_key_presses) // Bind key presses
         .bind(final_mouse_clicks) // Bind mouse clicks
         .bind(session_id)
         .execute(&state.db_pool)
         .await
         .map_err(|e| format!("Failed to update session end time and activity counts in DB: {}", e))?;
         println!("Ended session with ID: {} and saved activity counts.", session_id);
     } else {
         eprintln!("Warning: Could not find current session ID when stopping timer to save activity counts.");
     }
     *state.current_session_id.lock().await = None; // Clear current session ID
     *state.session_start_time.lock().await = None; // Clear start time
     // --- End Session Handling ---


     if let Some(tx) = state.command_tx.lock().await.take() { // Use take() to consume the sender
         if tx.send(TimerCommand::Stop).await.is_err() {
            // If sending fails, the task might have already stopped. Manually update status.
            eprintln!("Failed to send stop command or channel closed. Forcing status update.");
            *status = TimerStatus::Stopped;
            app_handle.emit("timer_status_update", TimerStatus::Stopped).unwrap();
         }
         // Task will update status upon receiving command if send was successful
     } else {
         // Should not happen if timer is running/paused, but handle defensively
         println!("Command channel not found while stopping. Forcing status update.");
         *status = TimerStatus::Stopped;
         app_handle.emit("timer_status_update", TimerStatus::Stopped).unwrap();
     }

     Ok(())
}

// Tauri command to pause the timer
// No session changes needed on pause, but ensure status update happens
#[tauri::command]
async fn pause_timer(state: State<'_, AppState>, app_handle: AppHandle) -> Result<(), String> { // Added app_handle back
    let status = state.timer_status.lock().await;
    if *status != TimerStatus::Running {
        return Err("Timer is not running.".to_string());
    }
    println!("Pausing timer...");

    if let Some(tx) = state.command_tx.lock().await.as_ref() {
        tx.send(TimerCommand::Pause)
            .await
            .map_err(|e| format!("Failed to send pause command: {}", e))?;
        // Status update is handled by the task upon receiving command
        Ok(())
    } else {
        // If channel is gone, task likely stopped unexpectedly. Update status.
        println!("Command channel not found while pausing. Forcing status update.");
        drop(status); // Release lock before acquiring again
        *state.timer_status.lock().await = TimerStatus::Stopped; // Go to stopped if task died
        app_handle.emit("timer_status_update", TimerStatus::Stopped).unwrap();
        Err("Timer command channel not found, task may have stopped.".to_string())
    }
}

// Tauri command to resume the timer
// No session changes needed on resume, but ensure status update happens
#[tauri::command]
async fn resume_timer(state: State<'_, AppState>, app_handle: AppHandle) -> Result<(), String> { // Added app_handle back
    let status = state.timer_status.lock().await;
    if *status != TimerStatus::Paused {
        return Err("Timer is not paused.".to_string());
    }
     println!("Resuming timer...");

    if let Some(tx) = state.command_tx.lock().await.as_ref() {
        tx.send(TimerCommand::Resume)
            .await
            .map_err(|e| format!("Failed to send resume command: {}", e))?;
        // Status update is handled by the task upon receiving command
        Ok(())
    } else {
         // If channel is gone, task likely stopped unexpectedly. Update status.
         println!("Command channel not found while resuming. Forcing status update.");
         drop(status); // Release lock before acquiring again
         *state.timer_status.lock().await = TimerStatus::Stopped; // Go to stopped if task died
         app_handle.emit("timer_status_update", TimerStatus::Stopped).unwrap();
         Err("Timer command channel not found, task may have stopped.".to_string())
    }
}

// Tauri command to get the current timer status
#[tauri::command]
async fn get_timer_status(state: State<'_, AppState>) -> Result<TimerStatus, String> {
    Ok(state.timer_status.lock().await.clone())
}


// --- NEW COMMAND: get_screenshot_data ---
#[tauri::command]
async fn get_screenshot_data(
    id: String, // Receive UUID as String from JS
    state: State<'_, AppState>,
) -> Result<String, String> {
    let screenshot_uuid = Uuid::parse_str(&id)
        .map_err(|_| "Invalid UUID format".to_string())?;

    println!("Fetching screenshot data for ID: {}", screenshot_uuid);

    // Use query() function (runtime check) for fetching screenshot data
    let record = sqlx::query("SELECT image_data FROM screenshots WHERE id = $1") // Use query()
        .bind(screenshot_uuid)
        .fetch_optional(&state.db_pool)
        .await
        .map_err(|e| format!("Database error fetching screenshot: {}", e))?;

    if let Some(rec) = record {
        // Need to get the column data using column name or index with query()
        use sqlx::Row;
        let image_data: Vec<u8> = rec.try_get("image_data")
            .map_err(|e| format!("Failed to get image_data column: {}", e))?;
        // Encode bytea data as Base64
        let base64_image = BASE64_STANDARD.encode(&image_data);
        Ok(format!("data:image/png;base64,{}", base64_image)) // Return data URI
    } else {
        Err(format!("Screenshot with ID {} not found", screenshot_uuid))
    }
}

// --- NEW COMMAND: get_elapsed_time ---
#[tauri::command]
async fn get_elapsed_time(state: State<'_, AppState>) -> Result<u64, String> {
    let status = state.timer_status.lock().await.clone();
    let start_time_opt = *state.session_start_time.lock().await;

    match status {
        TimerStatus::Running | TimerStatus::Paused => {
            if let Some(start_time) = start_time_opt {
                let now = Utc::now();
                let duration = now.signed_duration_since(start_time);
                // Ensure duration is non-negative before converting
                Ok(duration.num_seconds().max(0) as u64)
            } else {
                // Should not happen if running/paused, but return 0 defensively
                println!("Warning: Timer is running/paused but session start time is missing.");
                Ok(0)
            }
        }
        TimerStatus::Stopped => Ok(0), // Return 0 if stopped
    }
}

// Tauri command to intentionally cause a panic for Sentry testing
#[tauri::command]
fn test_sentry_panic() {
    sentry::capture_message("test", sentry::Level::Info);
}

// --- NEW COMMAND: get_activity_data ---
#[tauri::command]
fn get_activity_data(state: State<'_, AppState>) -> Result<ActivityData, String> {
    // Directly use the helper function from the module
    Ok(get_current_counts(&state.activity_counters))
}


// Function to set up the database (create tables if not exists)
async fn setup_database(pool: &Pool<Postgres>) -> Result<(), sqlx::Error> {
    println!("Setting up database tables if they don't exist...");
    // Use query() function
    sqlx::query("CREATE EXTENSION IF NOT EXISTS \"uuid-ossp\";") // Use query()
        .execute(pool)
        .await?;

    // Create sessions table
    // Use query() function
    sqlx::query( // Use query()
        r#"
        CREATE TABLE IF NOT EXISTS sessions (
            id UUID PRIMARY KEY,
            start_time TIMESTAMPTZ NOT NULL,
            end_time TIMESTAMPTZ NULL -- Nullable for ongoing sessions
        );
        "#
    )
    .execute(pool)
    .await?;
    println!("Table 'sessions' ensured.");

    // Add activity count columns to sessions table if they don't exist
    sqlx::query(
        r#"
        DO $$
        BEGIN
            IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name='sessions' AND column_name='key_press_count') THEN
                ALTER TABLE sessions ADD COLUMN key_press_count INTEGER NULL;
            END IF;
            IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name='sessions' AND column_name='mouse_click_count') THEN
                ALTER TABLE sessions ADD COLUMN mouse_click_count INTEGER NULL;
            END IF;
        END $$;
        "#
    ).execute(pool).await?;
    println!("Columns 'key_press_count' and 'mouse_click_count' ensured in 'sessions'.");


    // Create screenshots table (if not exists)
    // Use query() function and add new columns
    sqlx::query( // Use query()
        r#"
        CREATE TABLE IF NOT EXISTS screenshots (
            id UUID PRIMARY KEY,
            capture_time TIMESTAMPTZ NOT NULL,
            image_data BYTEA NOT NULL,
            session_id UUID NULL, -- Initially allow NULL, FK added below
            monitor_count INTEGER NULL, -- Added monitor count
            open_windows TEXT[] NULL -- Added open windows list (PostgreSQL array)
        );
        "#
    )
    .execute(pool)
    .await?;
    println!("Table 'screenshots' ensured.");


    // Use ALTER TABLE to add the session_id column if missing
    // Use query() function
     sqlx::query( // Use query()
         r#"
         DO $$
         BEGIN
             IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name='screenshots' AND column_name='session_id') THEN
                 ALTER TABLE screenshots ADD COLUMN session_id UUID;
             END IF;
         END $$;
         "#
     ).execute(pool).await?;
     println!("Column 'session_id' ensured in 'screenshots'.");

    // Use ALTER TABLE to add the monitor_count column if missing
    // Use query() function
    sqlx::query( // Use query()
        r#"
        DO $$
        BEGIN
            IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name='screenshots' AND column_name='monitor_count') THEN
                ALTER TABLE screenshots ADD COLUMN monitor_count INTEGER;
            END IF;
        END $$;
        "#
    ).execute(pool).await?;
    println!("Column 'monitor_count' ensured in 'screenshots'.");

    // Use ALTER TABLE to add the open_windows column if missing
    // Use query() function
    sqlx::query( // Use query()
        r#"
        DO $$
        BEGIN
            IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name='screenshots' AND column_name='open_windows') THEN
                ALTER TABLE screenshots ADD COLUMN open_windows TEXT[]; -- PostgreSQL array type
            END IF;
        END $$;
        "#
    ).execute(pool).await?;
    println!("Column 'open_windows' ensured in 'screenshots'.");


     // Add FK constraint separately to handle potential timing issues or existing data
     // This might fail if there are existing screenshots without a valid session_id.
     // Consider data migration strategy for production.
     // Use query() function
     sqlx::query( // Use query()
         r#"
         DO $$
         BEGIN
             IF NOT EXISTS (
                 SELECT 1 FROM information_schema.table_constraints
                 WHERE constraint_name='fk_session' AND table_name='screenshots'
             ) THEN
                 ALTER TABLE screenshots
                 ADD CONSTRAINT fk_session
                 FOREIGN KEY (session_id) REFERENCES sessions(id)
                 ON DELETE SET NULL; -- Or CASCADE depending on desired behavior
             END IF;
         END $$;
         "#
     ).execute(pool).await?;
     println!("Foreign key 'fk_session' ensured on 'screenshots'.");


    println!("Database setup complete.");
    Ok(())
}

fn main() {
    // Initialize Sentry
    let _guard = sentry::init(("https://6d8ed92c0ada0a87a6fd9c785b1fac0e@sen.newhoopla.com/10", sentry::ClientOptions {
      release: sentry::release_name!(),
      ..Default::default()
    }));

    // Load environment variables from .env file
    dotenvy::dotenv().expect("Failed to load .env file");

    // Set up the database connection pool
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL must be set in .env file");
    let pool_options = PgPoolOptions::new()
        .max_connections(5); // Adjust pool size as needed

    // We need to run the async database setup within a tokio runtime
    // Tauri's main thread isn't async by default before run()
    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
    let db_pool = rt.block_on(async {
        let pool = pool_options
            .connect(&database_url)
            .await
            .expect("Failed to create Postgres connection pool");
        setup_database(&pool)
            .await
            .expect("Failed to setup database");
        pool
    });

    // Initialize the application state
    let app_state = AppState {
        db_pool,
        timer_status: Arc::new(Mutex::new(TimerStatus::Stopped)),
        command_tx: Arc::new(Mutex::new(None)),
        current_session_id: Arc::new(Mutex::new(None)), // Initialize new state field
        session_start_time: Arc::new(Mutex::new(None)), // Initialize new state field
        activity_counters: Arc::new(ActivityCounters::default()), // Initialize activity counters
        is_session_active: Arc::new(AtomicBool::new(false)), // Initialize session active flag
    };

    // --- Spawn Activity Monitor Thread ---
    // rdev::listen is blocking, so it needs its own dedicated thread, not a tokio task.
    let activity_counters_clone = Arc::clone(&app_state.activity_counters);
    let is_session_active_clone = Arc::clone(&app_state.is_session_active); // Clone the flag
    std::thread::spawn(move || {
        activity_listen(activity_counters_clone, is_session_active_clone); // Pass the flag
    });
    // --- End Spawn Activity Monitor Thread ---

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            println!("Another instance detected. Focusing main window.");
            // Use get_webview_window instead of get_window
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        // Removed tauri_plugin_shell as it's not used and wasn't added as a dependency
        .plugin(tauri_plugin_opener::init())
        .manage(app_state) // Add the state to Tauri
        .invoke_handler(tauri::generate_handler![
            start_timer,
            stop_timer,
            pause_timer,
            resume_timer,
            get_timer_status,
            get_elapsed_time, // Added
            get_screenshot_data, // Added
            test_sentry_panic,
            get_activity_data // Added activity data command
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// Ensure the lib entry point is present if you have a lib target (common in tauri plugins or complex apps)
// --- Mobile Stubs ---
#[cfg(target_os = "android")]
fn init_logging() {
    android_logger::init_once(
        android_logger::Config::default()
            .with_min_level(log::Level::Trace)
            .with_tag("myapp"),
    );
}

#[cfg(target_os = "ios")]
fn init_logging() {
    oslog::OsLogger::new("com.myapp.dev")
        .level_filter(log::LevelFilter::Trace)
        .init()
        .unwrap();
}

#[cfg(mobile)]
#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn Java_com_myapp_dev_MainActivity_initMobile() {
    init_logging();
    // Add mobile specific initialization here
}

// Example lib function (if needed)
pub fn add(left: usize, right: usize) -> usize {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);

    }
}
