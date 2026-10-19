use gpui::actions;

// All keyboard-driven actions for RustyEdit. Deliberately plain, unit-style
// actions (no parameters) so they work with GPUI's simple `actions!` macro;
// rating 0-5 therefore gets one action per digit rather than a single
// parameterized action.
actions!(
    rustyedit,
    [
        Quit,
        NextImage,
        PrevImage,
        FlagPick,
        FlagReject,
        FlagUnflag,
        Rating0,
        Rating1,
        Rating2,
        Rating3,
        Rating4,
        Rating5,
        LabelRed,
        LabelYellow,
        LabelGreen,
        LabelBlue,
        GoLibrary,
        GoDevelop,
        GoExport,
        ToggleInfo,
        CycleFilter,
        ToggleZoom,
        ImportFolder,
        OpenSettings,
        CycleLayout,
        CloseSettings,
    ]
);
