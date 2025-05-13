import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { TimerStatus, ActivityData } from "../types/timer";

interface UseTimerResult {
  timerStatus: TimerStatus;
  elapsedTime: number;
  lastError: string | null;
  lastScreenshots: (string | null)[];
  currentDateTime: Date;
  activityData: ActivityData | null;
  handleStart: () => Promise<void>;
  handleStop: () => Promise<void>;
  handlePause: () => Promise<void>;
  handleResume: () => Promise<void>;
}

export function useTimer(): UseTimerResult {
  const [timerStatus, setTimerStatus] = useState<TimerStatus>(TimerStatus.Stopped);
  const [elapsedTime, setElapsedTime] = useState<number>(0);
  const [lastError, setLastError] = useState<string | null>(null);
  const [lastScreenshots, setLastScreenshots] = useState<(string | null)[]>([null, null]);
  const [currentDateTime, setCurrentDateTime] = useState(new Date());
  const [activityData, setActivityData] = useState<ActivityData | null>(null);
  const intervalRef = useRef<NodeJS.Timeout | null>(null);
  const activityIntervalRef = useRef<NodeJS.Timeout | null>(null);

  // Function to fetch both status and elapsed time
  const fetchInitialState = async () => {
    setLastError(null);
    try {
      const status = await invoke<TimerStatus>("get_timer_status");
      setTimerStatus(status);
      if (status === TimerStatus.Running || status === TimerStatus.Paused) {
        const time = await invoke<number>("get_elapsed_time");
        setElapsedTime(time);
      } else {
        setElapsedTime(0);
      }
    } catch (err) {
      console.error("Error getting initial state:", err);
      setLastError(`Error getting initial state: ${err}`);
      setElapsedTime(0);
    }
  };

  // Fetch initial state and set up listeners
  useEffect(() => {
    fetchInitialState();

    const unlistenStatus = listen<TimerStatus>("timer_status_update", (event) => {
      const newStatus = event.payload;
      setTimerStatus(newStatus);
      setLastError(null);

      if (newStatus === TimerStatus.Stopped) {
        setElapsedTime(0);
        setLastScreenshots([null, null]);
        setActivityData({ key_presses: 0, mouse_clicks: 0 });
      } else if (newStatus === TimerStatus.Paused) {
        invoke<number>("get_elapsed_time")
          .then(setElapsedTime)
          .catch((err) => {
            setLastError(`Error getting time after pause: ${err}`);
          });
      } else if (newStatus === TimerStatus.Running) {
        invoke<number>("get_elapsed_time")
          .then(setElapsedTime)
          .catch((err) => {
            setLastError(`Error getting time after start/resume: ${err}`);
          });
      }
    });

    const unlistenError = listen<string>("screenshot_error", (event) => {
      setLastError(`Screenshot Error: ${event.payload}`);
    });

    const unlistenNewScreenshot = listen<string>("new_screenshot", async (event) => {
      const screenshotId = event.payload;
      setLastError(null);
      try {
        const dataUri = await invoke<string>("get_screenshot_data", { id: screenshotId });
        setLastScreenshots((prev) => [prev[1], dataUri]);
      } catch (err) {
        setLastError(`Error fetching screenshot ${screenshotId}: ${err}`);
      }
    });

    return () => {
      unlistenStatus.then((f) => f());
      unlistenError.then((f) => f());
      unlistenNewScreenshot.then((f) => f());
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
      }
      if (activityIntervalRef.current) {
        clearInterval(activityIntervalRef.current);
      }
    };
  }, []);

  // Effect to update the current date/time every second
  useEffect(() => {
    const dateTimeInterval = setInterval(() => {
      setCurrentDateTime(new Date());
    }, 1000);

    return () => {
      clearInterval(dateTimeInterval);
    };
  }, []);

  // Effect to manage the timer interval based on status
  useEffect(() => {
    if (intervalRef.current) {
      clearInterval(intervalRef.current);
      intervalRef.current = null;
    }

    if (timerStatus === TimerStatus.Running) {
      intervalRef.current = setInterval(() => {
        setElapsedTime((prevTime) => prevTime + 1);
      }, 1000);
    }

    return () => {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    };
  }, [timerStatus]);

  // Effect to fetch activity data periodically
  useEffect(() => {
    const fetchActivityData = async () => {
      try {
        const data = await invoke<ActivityData>("get_activity_data");
        setActivityData(data);
      } catch (err) {
        // Optionally set an error state specific to activity data
      }
    };

    fetchActivityData();
    activityIntervalRef.current = setInterval(fetchActivityData, 5000);

    return () => {
      if (activityIntervalRef.current) {
        clearInterval(activityIntervalRef.current);
      }
    };
  }, []);

  const handleStart = async () => {
    setLastError(null);
    try {
      await invoke("start_timer");
    } catch (err) {
      setLastError(`Error starting timer: ${err}`);
    }
  };

  const handleStop = async () => {
    setLastError(null);
    try {
      await invoke("stop_timer");
    } catch (err) {
      setLastError(`Error stopping timer: ${err}`);
    }
  };

  const handlePause = async () => {
    setLastError(null);
    try {
      await invoke("pause_timer");
    } catch (err) {
      setLastError(`Error pausing timer: ${err}`);
    }
  };

  const handleResume = async () => {
    setLastError(null);
    try {
      await invoke("resume_timer");
    } catch (err) {
      setLastError(`Error resuming timer: ${err}`);
    }
  };

  return {
    timerStatus,
    elapsedTime,
    lastError,
    lastScreenshots,
    currentDateTime,
    activityData,
    handleStart,
    handleStop,
    handlePause,
    handleResume,
  };
}
