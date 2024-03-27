

//-------------------------------------------------------------------------------------------------------------------

/// Run condition that returns `true` if [`WorldSwapStatus`] equals [`Suspended`](WorldSwapStatus::Suspended).
pub fn suspended(status: Res<WorldSwapStatus>) -> bool
{
    *status == WorldSwapStatus::Suspended
}

//-------------------------------------------------------------------------------------------------------------------

/// Run condition that returns `true` if [`WorldSwapStatus`] equals [`Background`](WorldSwapStatus::Background).
pub fn in_background(status: Res<WorldSwapStatus>) -> bool
{
    *status == WorldSwapStatus::Background
}

//-------------------------------------------------------------------------------------------------------------------

/// Run condition that returns `true` if [`WorldSwapStatus`] equals [`Foreground`](WorldSwapStatus::Foreground).
pub fn in_foreground(status: Res<WorldSwapStatus>) -> bool
{
    *status == WorldSwapStatus::Foreground
}

//-------------------------------------------------------------------------------------------------------------------

/// Run condition that returns `true` if [`WorldSwapStatus`] just entered [`Background`](WorldSwapStatus::Background).
///
/// Note that this only detects entering the background the first time the world updates, and if the world updated while
/// not in the background. If you use [`BackgroundTickRate::Never`], then this won't detect movement between foreground
/// and background (and the other tick rate options may also not detect it if you swap back and forth too fast).
pub fn entered_background(mut prev: Local<Option<WorldSwapStatus>>, status: Res<WorldSwapStatus>) -> bool
{
    let last = *prev;
    *prev = Some(*status);

    if *status != WorldSwapStatus::Background { return false; }
    if last == Some(*status) { return false; }
    true
}

//-------------------------------------------------------------------------------------------------------------------

/// Run condition that returns `true` if [`WorldSwapStatus`] just entered [`Foreground`](WorldSwapStatus::Foreground).
///
/// Note that this only detects entering the foreground the first time the world updates, and if the world updated while
/// not in the foreground. If you use [`BackgroundTickRate::Never`], then this won't detect movement between background
/// and foreground (and the other tick rate options may also not detect it if you swap back and forth too fast).
pub fn entered_foreground(mut prev: Local<Option<WorldSwapStatus>>, status: Res<WorldSwapStatus>) -> bool
{
    let last = *prev;
    *prev = Some(*status);

    if *status != WorldSwapStatus::Foreground { return false; }
    if last == Some(*status) { return false; }
    true
}

//-------------------------------------------------------------------------------------------------------------------
