//! Profiles and app lock (v1.0). Not collection writes: no oplog, no undo; user only.

use crate::v1::common::Empty;
use crate::v1::session::*;

methods! {
    /// Profiles of this installation.
    ProfilesList = "profiles.list", Query, "profiles", "profiles:read", Public, undo: false, Empty => ProfileList;
    /// Creates a profile (an empty collection of its own).
    ProfilesCreate = "profiles.create", Command, "profiles", "profiles:write", Public, undo: false, CreateProfile => ProfileDto;
    /// Renames a profile.
    ProfilesRename = "profiles.rename", Command, "profiles", "profiles:write", Public, undo: false, RenameProfile => ProfileDto;
    /// Deletes a profile and its collection folder (not the active one; not undoable).
    ProfilesDelete = "profiles.delete", Command, "profiles", "profiles:write", Public, undo: false, DeleteProfile => Empty;
    /// Makes a profile the one to open; the app restarts on it.
    ProfilesSwitch = "profiles.switch", Command, "profiles", "profiles:write", Public, undo: false, SwitchProfile => ProfileSwitched;
    /// App lock state of the active profile.
    LockStatus = "lock.status", Query, "profiles", "lock:read", Public, undo: false, Empty => LockState;
    /// Sets or changes the PIN/password.
    LockSet = "lock.set", Command, "profiles", "lock:write", Public, undo: false, SetLock => LockState;
    /// Changes the idle timeout.
    LockConfigure = "lock.configure", Command, "profiles", "lock:write", Public, undo: false, ConfigureLock => LockState;
    /// Removes the lock.
    LockClear = "lock.clear", Command, "profiles", "lock:write", Public, undo: false, ClearLock => LockState;
    /// Unlocks with the PIN/password.
    LockUnlock = "lock.unlock", Command, "profiles", "lock:write", Public, undo: false, Unlock => LockState;
    /// Locks now.
    LockLock = "lock.lock", Command, "profiles", "lock:write", Public, undo: false, Empty => LockState;
    /// The user is active (keyboard/mouse): restarts the idle timer.
    LockActivity = "lock.activity", Command, "profiles", "lock:write", Public, undo: false, Empty => Empty;
}
