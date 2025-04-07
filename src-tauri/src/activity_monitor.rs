use rdev::{listen as rdev_listen, Event, EventType};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering}, // Added AtomicBool
    Arc,
};
// Removed unused tokio::sync::Mutex import

/// Holds atomic counters for different types of user activity.
/// Wrapped in Arc<Mutex<...>> for safe sharing across threads.
#[derive(Default, Debug)]
pub struct ActivityCounters {
    pub key_presses: AtomicUsize,
    pub mouse_clicks: AtomicUsize,
    // Add more counters as needed (e.g., mouse_movement_distance)
}

/// Data structure sent to the frontend.
#[derive(Serialize, Clone, Debug)]
pub struct ActivityData {
    key_presses: usize,
    mouse_clicks: usize,
}

/// Listens for global input events and updates the counters if the session is active.
/// This function is intended to be run in a separate thread.
pub fn listen(counters: Arc<ActivityCounters>, is_session_active: Arc<AtomicBool>) {
    let callback = move |event: Event| {
        // Only count if the session is active
        if !is_session_active.load(Ordering::Relaxed) {
            return;
        }

        match event.event_type {
            EventType::KeyPress(_) => {
                counters.key_presses.fetch_add(1, Ordering::Relaxed);
                // Optional: Log the key press (be mindful of privacy)
                // println!("Key Press: {:?}", event.name);
            }
            EventType::ButtonPress(_) => {
                counters.mouse_clicks.fetch_add(1, Ordering::Relaxed);
                // Optional: Log the mouse click
                // println!("Mouse Click: {:?}", event.button);
            }
            // Add cases for other events if needed (e.g., MouseMove, Wheel)
            _ => {} // Ignore other event types for now
        }
    };

    println!("Starting activity monitor thread..."); // Log start

    // rdev::listen blocks the current thread.
    if let Err(error) = rdev_listen(callback) {
        eprintln!("Error starting rdev listener: {:?}", error);
        // Consider more robust error handling or reporting here
    }

    println!("Activity monitor thread finished."); // Log end (might not be reached if listen runs indefinitely)
}

// Helper function to get current counts (might be used by the Tauri command)
pub fn get_current_counts(counters: &ActivityCounters) -> ActivityData {
    ActivityData {
        key_presses: counters.key_presses.load(Ordering::Relaxed),
        mouse_clicks: counters.mouse_clicks.load(Ordering::Relaxed),
    } // Removed semicolon to return the struct
} // Closing brace remains
