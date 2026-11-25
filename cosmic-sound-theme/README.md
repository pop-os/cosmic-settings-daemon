# cosmic-sound-theme

Library for playing sounds from FD.o sound themes.

```rs
cosmic_sound_theme::select(&["battery-low"])
    // Override the sound themes searched
    .themes(&["cosmic", "freedesktop"])
    // Equivalent to `media_role("Notification")`
    .notification_role()
    // Defaults to `Stereo`
    .output_profile(cosmic_sound_theme::OutputProfile::Surround51)
    // Play with `pw-play`
    .play()
    .await;
```
