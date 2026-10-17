#pragma once

// StoryGraph's internal reading-status IDs, mirrored from the Rust CLI's
// ReadingStatus enum (cli/src/storygraph.rs). Passed to the CLI as `--status`.
enum StoryGraphStatus {
  StoryGraphStatusToRead = 1,
  StoryGraphStatusCurrentlyReading = 2,
  StoryGraphStatusRead = 3,
  StoryGraphStatusPaused = 4,
  StoryGraphStatusDidNotFinish = 5,
  StoryGraphStatusRereading = 6,
};
