import React, { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import './App.css'; // Assuming basic styling exists

// Define the TimerStatus enum matching the backend
enum TimerStatus {
  Stopped = 'Stopped',
  Running = 'Running',
  Paused = 'Paused',
}

// Helper function to format seconds into HH:MM:SS
const formatTime = (totalSeconds: number): string => {
  // Ensure totalSeconds is an integer
  totalSeconds = Math.floor(totalSeconds);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  return `${hours.toString().padStart(2, '0')}:${minutes.toString().padStart(2, '0')}:${seconds.toString().padStart(2, '0')}`;
};


function App() {
  const [timerStatus, setTimerStatus] = useState<TimerStatus>(TimerStatus.Stopped);
  const [elapsedTime, setElapsedTime] = useState<number>(0); // Time in seconds
  const [lastError, setLastError] = useState<string | null>(null);
  const [lastScreenshots, setLastScreenshots] = useState<(string | null)[]>([null, null]); // Store last 2 screenshot data URIs
  const intervalRef = useRef<NodeJS.Timeout | null>(null);


  // Function to fetch both status and elapsed time
  const fetchInitialState = async () => {
    setLastError(null);
    try {
      const status = await invoke<TimerStatus>('get_timer_status');
      setTimerStatus(status);
      if (status === TimerStatus.Running || status === TimerStatus.Paused) {
        // Assuming backend provides elapsed time in seconds
        const time = await invoke<number>('get_elapsed_time');
        setElapsedTime(time);
      } else {
        setElapsedTime(0);
      }
    } catch (err) {
      console.error("Error getting initial state:", err);
      setLastError(`Error getting initial state: ${err}`);
      setElapsedTime(0); // Reset time on error
    }
  };


  // Fetch initial state and set up listeners
  useEffect(() => {
    fetchInitialState();

    // Listen for status updates from the backend
    const unlistenStatus = listen<TimerStatus>('timer_status_update', (event) => {
      const newStatus = event.payload;
      console.log('Received timer_status_update:', newStatus);
      setTimerStatus(newStatus);
      setLastError(null); // Clear error on successful status change

      // If stopped, reset time immediately on frontend for responsiveness
      if (newStatus === TimerStatus.Stopped) {
        setElapsedTime(0);
      }
      // If paused, fetch the exact time it was paused at
      else if (newStatus === TimerStatus.Paused) {
         invoke<number>('get_elapsed_time')
           .then(setElapsedTime)
           .catch(err => {
                console.error("Error getting elapsed time after pause:", err);
                setLastError(`Error getting time after pause: ${err}`);
           });
      }
       // If running (either from start or resume), fetch time to be sure
       else if (newStatus === TimerStatus.Running) {
         invoke<number>('get_elapsed_time')
           .then(setElapsedTime)
           .catch(err => {
                console.error("Error getting elapsed time after start/resume:", err);
                setLastError(`Error getting time after start/resume: ${err}`);
           });
       }
    });

     // Listen for screenshot errors from the backend
     const unlistenError = listen<string>('screenshot_error', (event) => {
       console.error('Received screenshot_error:', event.payload);
       setLastError(`Screenshot Error: ${event.payload}`);
     });

     // Listen for new screenshots
     const unlistenNewScreenshot = listen<string>('new_screenshot', async (event) => {
        const screenshotId = event.payload;
        console.log('Received new_screenshot event with ID:', screenshotId);
        setLastError(null); // Clear previous errors
        try {
            const dataUri = await invoke<string>('get_screenshot_data', { id: screenshotId });
            console.log('Fetched screenshot data URI (truncated):', dataUri.substring(0, 50) + '...');
            setLastScreenshots(prev => [prev[1], dataUri]); // Add new, remove oldest
        } catch (err) {
            console.error("Error getting screenshot data:", err);
            setLastError(`Error fetching screenshot ${screenshotId}: ${err}`);
        }
     });


     // Cleanup listeners on component unmount
     return () => {
       unlistenStatus.then(f => f());
       unlistenError.then(f => f());
       unlistenNewScreenshot.then(f => f()); // Cleanup screenshot listener
       if (intervalRef.current) {
         clearInterval(intervalRef.current); // Clear interval on unmount
       }
     };
  }, []); // Empty dependency array ensures this runs only once on mount


  // Effect to manage the timer interval based on status
  useEffect(() => {
    if (intervalRef.current) {
      clearInterval(intervalRef.current); // Clear previous interval first
      intervalRef.current = null;
    }

    if (timerStatus === TimerStatus.Running) {
      // Start interval only when running
      intervalRef.current = setInterval(() => {
        setElapsedTime((prevTime) => prevTime + 1);
      }, 1000);
    }

    // Cleanup interval on status change or unmount
    return () => {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    };
  }, [timerStatus]); // Rerun when timerStatus changes


  // Example function to trigger the panic
  async function triggerSentryTestPanic() {
  try {
    await invoke('test_sentry_panic');
    // This line won't be reached because the backend will panic
  } catch (error) {
    // The frontend might receive an error if Tauri handles the panic gracefully,
    // but the primary goal is to see the event in Sentry.
    console.error("Error invoking test_sentry_panic (this might be expected):", error);
  }
}
  const handleStart = async () => {
    setLastError(null);
    try {
      triggerSentryTestPanic();
      await invoke('start_timer');
      // Status update will come via the listener
    } catch (err) {
      console.error("Error starting timer:", err);
      setLastError(`Error starting timer: ${err}`);
    }
  };

  const handleStop = async () => {
    setLastError(null);
    try {
      await invoke('stop_timer');
      // Status update will come via the listener
    } catch (err) {
      console.error("Error stopping timer:", err);
      setLastError(`Error stopping timer: ${err}`);
    }
  };

  const handlePause = async () => {
    setLastError(null);
    try {
      await invoke('pause_timer');
      // Status update will come via the listener
    } catch (err) {
      console.error("Error pausing timer:", err);
      setLastError(`Error pausing timer: ${err}`);
    }
  };

  const handleResume = async () => {
    setLastError(null);
    try {
      await invoke('resume_timer');
      // Status update will come via the listener
    } catch (err) {
      console.error("Error resuming timer:", err);
      setLastError(`Error resuming timer: ${err}`);
    }
  };

  return (
    <div className="container">
      <h1>Screenshot Timer</h1>
      <div className="timer-display">
         <p>Status: <strong>{timerStatus}</strong></p>
         <p>Elapsed Time: <strong>{formatTime(elapsedTime)}</strong></p>
      </div>

      {lastError && <p className="error-message">Error: {lastError}</p>}

      <div className="button-group">
        {timerStatus === TimerStatus.Stopped && (
          <button onClick={handleStart}>Start</button>
        )}
        {timerStatus === TimerStatus.Running && (
          <>
            <button onClick={handlePause}>Pause</button>
            <button onClick={handleStop}>Stop</button>
          </>
        )}
        {timerStatus === TimerStatus.Paused && (
          <>
            <button onClick={handleResume}>Resume</button>
            <button onClick={handleStop}>Stop</button>
          </>
        )}
      </div>

      {/* Screenshot Display Area */}
      <div className="screenshot-display">
        <h2>Last Screenshots</h2>
        <div className="screenshot-images">
          {lastScreenshots[0] && (
            <img src={lastScreenshots[0]} alt="Previous screenshot" className="screenshot-image" />
          )}
          {lastScreenshots[1] && (
            <img src={lastScreenshots[1]} alt="Latest screenshot" className="screenshot-image" />
          )}
          {!lastScreenshots[0] && !lastScreenshots[1] && (
            <p>No screenshots captured yet in this session.</p>
          )}
        </div>
      </div>
    </div>
  );
}

export default App;
