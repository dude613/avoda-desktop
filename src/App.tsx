import React, { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Button } from "@/components/ui/button";
import './App.css'; // Now imports Tailwind

export enum TimerStatus {
  Stopped = 'Stopped',
  Running = 'Running',
  Paused = 'Paused',
}

// Re-added formatTime helper function
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
  const [currentDateTime, setCurrentDateTime] = useState(new Date()); // State for live date/time
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

      // If stopped, reset time and clear screenshots
      if (newStatus === TimerStatus.Stopped) {
        setElapsedTime(0);
        setLastScreenshots([null, null]); // Clear screenshots
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


  // Effect to update the current date/time every second
  useEffect(() => {
    const dateTimeInterval = setInterval(() => {
      setCurrentDateTime(new Date());
    }, 1000); // Update every second

    // Cleanup interval on unmount
    return () => {
      clearInterval(dateTimeInterval);
    };
  }, []); // Run only once on mount


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


  async function triggerSentryTestPanic() {
    invoke('test_sentry_panic');
  }
  const handleStart = async () => {
    setLastError(null);
    try {
      await invoke('start_timer');
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

  // Base button classes
  const btnClasses = "px-4 py-2 rounded text-white font-semibold shadow disabled:opacity-50 focus:outline-none focus:ring-2 focus:ring-offset-2 hover:cursor-pointer";
  const btnPrimary = `${btnClasses} bg-blue-500 hover:bg-blue-600 focus:ring-blue-400`;
  const btnSecondary = `${btnClasses} bg-gray-500 hover:bg-gray-600 focus:ring-gray-400`;
  const btnWarning = `${btnClasses} bg-yellow-500 hover:bg-yellow-600 focus:ring-yellow-400`;


  return (
    // Added relative positioning for the absolute date/time display
    <div className="relative container mx-auto p-4 max-w-2xl min-h-screen flex flex-col">

      {/* Date/Time Display */}
      <div className="absolute top-4 right-4 text-sm text-gray-600">
        {currentDateTime.toLocaleString()}
      </div>

      {/* Header */}
      <h1 className="text-3xl font-bold text-center mb-4 mt-2 text-gray-800">Screenshot Timer</h1>

      {/* Status and Timer Display */}
      <div className="text-center my-4 p-4 bg-gray-100 rounded-lg shadow-inner">
         <p className="text-gray-700">Status: <strong className="font-semibold text-gray-900">{timerStatus}</strong></p>
         <p className="mt-1">Elapsed Time: <strong className="font-mono text-2xl text-blue-600">{formatTime(elapsedTime)}</strong></p>
      </div>

      {/* Error Message */}
      {lastError && <p className="text-red-700 text-center my-2 p-3 bg-red-100 rounded border border-red-400 shadow">Error: {lastError}</p>}

      <div className="flex justify-center gap-4 my-6"> {/* Button group */}
        {timerStatus === TimerStatus.Stopped && (
          <Button onClick={handleStart} variant="default">Start</Button>
        )}
        {timerStatus === TimerStatus.Running && (
          <>
            <Button onClick={handlePause} variant="outline">Pause</Button>
            <Button onClick={handleStop} variant="destructive">Stop</Button>
          </>
        )}
        {timerStatus === TimerStatus.Paused && (
          <>
            <Button onClick={handleResume} variant="default">Resume</Button>
            <Button onClick={handleStop} variant="destructive">Stop</Button>
          </>
        )}
      </div>

      {/* Screenshot Display Area */}
      <div className="mt-8 pt-6 border-t border-gray-300 w-full text-center"> {/* Screenshot display container */}
        <h2 className="text-xl font-semibold mb-4">Last Screenshots</h2>
        <div className="flex justify-around items-center mt-4 min-h-[150px] bg-gray-50 p-3 rounded-md border border-gray-200"> {/* Images container */}
          {lastScreenshots[0] && (
            <img src={lastScreenshots[0]} alt="Previous screenshot" className="max-w-[45%] max-h-[200px] h-auto border border-gray-300 shadow-md rounded" />
          )}
          {lastScreenshots[1] && (
            <img src={lastScreenshots[1]} alt="Latest screenshot" className="max-w-[45%] max-h-[200px] h-auto border border-gray-300 shadow-md rounded" />
          )}
          {!lastScreenshots[0] && !lastScreenshots[1] && (
            <p className="text-gray-500">No screenshots captured yet in this session.</p>
          )}
        </div>
      </div>
    </div>
  );
}

export default App;
