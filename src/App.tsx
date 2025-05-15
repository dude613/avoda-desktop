import { Button } from "@/components/ui/button";
import "./App.css";
import { TimerStatus } from "./types/timer";
import { useState } from "react";
import { formatTime } from "./lib/formatTime";
import { useTimer } from "./hooks/useTimer";

function App() {
  const {
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
  } = useTimer();

  const [showScreenshots, setShowScreenshots] = useState(true);

  const toggleShowScreenshots = () => {
    setShowScreenshots((prev) => !prev);
  };

  return (
    <div className="relative container mx-auto p-4 max-w-2xl min-h-screen flex flex-col">
      {/* Date/Time Display */}
      <div className="absolute top-4 right-4 text-sm text-gray-600">
        {currentDateTime.toLocaleString()}
      </div>

      {/* Header */}
      <h1 className="text-3xl font-bold text-center mb-4 mt-2 text-gray-800">
        Screenshot Timer
      </h1>

      {/* Status, Timer, and Activity Display */}
      <div className="text-center my-4 p-4 bg-gray-100 rounded-lg shadow-inner">
        <p className="text-gray-700">
          Status:{" "}
          <strong className="font-semibold text-gray-900">{timerStatus}</strong>
        </p>
        <p className="mt-1">
          Elapsed Time:{" "}
          <strong className="font-mono text-2xl text-blue-600">
            {formatTime(elapsedTime)}
          </strong>
        </p>
        {/* Activity Data Display */}
        {activityData && (
          <div className="mt-2 text-sm text-gray-600">
            <span>
              Keys:{" "}
              <strong className="font-semibold text-gray-800">
                {activityData.key_presses}
              </strong>
            </span>
            <span className="ml-4">
              Clicks:{" "}
              <strong className="font-semibold text-gray-800">
                {activityData.mouse_clicks}
              </strong>
            </span>
          </div>
        )}
      </div>

      {/* Error Message */}
      {lastError && (
        <p className="text-red-700 text-center my-2 p-3 bg-red-100 rounded border border-red-400 shadow">
          Error: {lastError}
        </p>
      )}

      <div className="flex justify-center gap-4 my-6">
        {/* Button group */}
        {timerStatus === TimerStatus.Stopped && (
          <Button onClick={handleStart} variant="default">
            Start
          </Button>
        )}
        {timerStatus === TimerStatus.Running && (
          <>
            <Button onClick={handlePause} variant="outline">
              Pause
            </Button>
            <Button onClick={handleStop} variant="destructive">
              Stop
            </Button>
          </>
        )}
        {timerStatus === TimerStatus.Paused && (
          <>
            <Button onClick={handleResume} variant="default">
              Resume
            </Button>
            <Button onClick={handleStop} variant="destructive">
              Stop
            </Button>
          </>
        )}
      </div>

      {/* Screenshot Display Area */}
      <div className="mt-8 pt-6 border-t border-gray-300 w-full text-center">
        <div className="flex justify-between items-center mb-4">
          <h2 className="text-xl font-semibold">Last Screenshots</h2>
          <Button onClick={toggleShowScreenshots} variant="outline" size="sm">
            {showScreenshots ? "Hide" : "Show"} Screenshots
          </Button>
        </div>
        {showScreenshots && (
          <div className="flex justify-around items-center mt-4 min-h-[150px] bg-gray-50 p-3 rounded-md border border-gray-200">
            {/* Images container */}
            {lastScreenshots[0] && (
              <img
                src={lastScreenshots[0]}
                alt="Previous screenshot"
                className="max-w-[45%] max-h-[200px] h-auto border border-gray-300 shadow-md rounded"
              />
            )}
            {lastScreenshots[1] && (
              <img
                src={lastScreenshots[1]}
                alt="Latest screenshot"
                className="max-w-[45%] max-h-[200px] h-auto border border-gray-300 shadow-md rounded"
              />
            )}
            {!lastScreenshots[0] && !lastScreenshots[1] && (
              <p className="text-gray-500">
                No screenshots captured yet in this session.
              </p>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

export default App;
