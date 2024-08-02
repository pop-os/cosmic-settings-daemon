// SPDX-License-Identifier: MPL-2.0

use serde::{Deserialize, Serialize};

/// An operation which may be bound to a keyboard shortcut.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Action {
    /// Close the active window
    Close,

    /// Show a debug overlay, if enabled in the compositor build
    Debug,

    /// Disable a default shortcut binding
    Disable,

    /// Change focus to the window or workspace in the given direction
    Focus(FocusDirection),

    /// Change focus to the last workspace
    LastWorkspace,

    /// Maximize the active window
    Maximize,

    /// Migrate the active workspace to the next output
    MigrateWorkspaceToNextOutput,

    /// Migrate the active workspace to the output in the given direction
    MigrateWorkspaceToOutput(Direction),

    /// Migrate the active workspace to the previous output
    MigrateWorkspaceToPreviousOutput,

    /// Minimize the active window
    Minimize,

    /// Move a window in the given direction
    Move(Direction),

    /// Move a window to the last workspace
    MoveToLastWorkspace,

    /// Move a window to the next output
    MoveToNextOutput,

    /// Move a window to the next workspace
    MoveToNextWorkspace,

    /// Move a window to the given output
    MoveToOutput(Direction),

    /// Move a window to the previous output
    MoveToPreviousOutput,

    /// Move a window to the previous workspace
    MoveToPreviousWorkspace,

    /// Move a window to the given workspace
    MoveToWorkspace(u8),

    /// Change focus to the next output
    NextOutput,

    /// Change focus to the next workspace
    NextWorkspace,

    /// Change the orientation of a tiling group
    Orientation(Orientation),

    /// Change focus to the previous output
    PreviousOutput,

    /// Change focus to the previous workspace
    PreviousWorkspace,

    /// Resize the active window in a given direction
    Resizing(ResizeDirection),

    /// Move a window to the last workspace
    SendToLastWorkspace,

    /// Move a window to the next output
    SendToNextOutput,

    /// Move a window to the next workspace
    SendToNextWorkspace,

    /// Move a window to the output in the given direction
    SendToOutput(Direction),

    /// Move a window to the previous output
    SendToPreviousOutput,

    /// Move a window to the previous workspace
    SendToPreviousWorkspace,

    /// Move a window to the given workspace
    SendToWorkspace(u8),

    /// Swap positions of the active window with another
    SwapWindow,

    /// Move to an output in the given direction
    SwitchOutput(Direction),

    /// Perform a common system operation
    System(System),

    /// Execute a command with any given arguments
    Spawn(String),

    /// Stop the compositor
    Terminate,

    /// Toggle the orientation of a tiling group
    ToggleOrientation,

    /// Toggle window stacking for the active window
    ToggleStacking,

    /// Toggle the sticky state of the active window
    ToggleSticky,

    /// Toggle tiling mode of the active workspace
    ToggleTiling,

    /// Toggle between tiling and floating window states for the active window
    ToggleWindowFloating,

    /// Change focus to the given workspace ID
    Workspace(u8),
}

/// Common system operations which may be controlled by system commands
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum System {
    /// Opens the application library
    AppLibrary,
    /// Decreases screen brightness
    BrightnessDown,
    /// Increases screen brightness
    BrightnessUp,
    /// Opens the home folder in a system default file browser
    HomeFolder,
    /// Decreases keyboard brightness
    KeyboardBrightnessDown,
    /// Increases keyboard brightness
    KeyboardBrightnessUp,
    /// Opens the launcher
    Launcher,
    /// Locks the screen
    LockScreen,
    /// Mutes the active audio output
    Mute,
    /// Mutes the active microphone
    MuteMic,
    /// Plays and Pauses audio
    PlayPause,
    /// Goes to the next track
    PlayNext,
    /// Goes to the previous track
    PlayPrev,
    /// Takes a screenshot
    Screenshot,
    /// Opens the system default terminal
    Terminal,
    /// Lowers the volume of the active audio output
    VolumeLower,
    /// Raises the volume of the active audio output
    VolumeRaise,
    /// Opens the system default web browser
    WebBrowser,
    /// Opens the (alt+tab) window switcher
    WindowSwitcher,
    /// Opens the workspace overview
    WorkspaceOverview,
}

/// Defines the direction of an operation
#[derive(Copy, Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl std::ops::Not for Direction {
    type Output = Self;
    fn not(self) -> Self::Output {
        match self {
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
        }
    }
}

/// Defines the direction to focus towards
#[derive(Copy, Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum FocusDirection {
    Left,
    Right,
    Up,
    Down,
    In,
    Out,
}

/// Defines the direction to resize towards
#[derive(Copy, Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ResizeDirection {
    Inwards,
    Outwards,
}

/// Defines the edge of a window to resize from
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ResizeEdge {
    Bottom,
    BottomLeft,
    BottomRight,
    Left,
    Right,
    Top,
    TopLeft,
    TopRight,
}

/// Tiling orientation for a tiling window group
#[derive(Copy, Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

impl std::ops::Not for Orientation {
    type Output = Self;
    fn not(self) -> Self::Output {
        match self {
            Orientation::Horizontal => Orientation::Vertical,
            Orientation::Vertical => Orientation::Horizontal,
        }
    }
}
