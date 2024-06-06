// SPDX-License-Identifier: MPL-2.0

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Action {
    Terminate,
    Debug,
    Close,
    Workspace(u8),
    NextWorkspace,
    PreviousWorkspace,
    LastWorkspace,
    MoveToWorkspace(u8),
    MoveToNextWorkspace,
    MoveToPreviousWorkspace,
    MoveToLastWorkspace,
    SendToWorkspace(u8),
    SendToNextWorkspace,
    SendToPreviousWorkspace,
    SendToLastWorkspace,

    NextOutput,
    PreviousOutput,
    MoveToNextOutput,
    MoveToPreviousOutput,
    SendToNextOutput,
    SendToPreviousOutput,
    SwitchOutput(Direction),
    MoveToOutput(Direction),
    SendToOutput(Direction),

    MigrateWorkspaceToNextOutput,
    MigrateWorkspaceToPreviousOutput,
    MigrateWorkspaceToOutput(Direction),

    Focus(FocusDirection),
    Move(Direction),

    ToggleOrientation,
    Orientation(Orientation),

    ToggleStacking,
    ToggleTiling,
    ToggleWindowFloating,
    ToggleSticky,
    SwapWindow,

    Resizing(ResizeDirection),
    Minimize,
    Maximize,

    /// Perform a common system operation
    System(System),

    /// Execute a command with any given arguments
    Spawn(String),
}

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

#[derive(Copy, Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum FocusDirection {
    Left,
    Right,
    Up,
    Down,
    In,
    Out,
}

#[derive(Copy, Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum ResizeDirection {
    Inwards,
    Outwards,
}

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
