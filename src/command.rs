//!
//! Provides functionality for accessing and creating commands
//!

use std::marker::PhantomData;
use std::ffi::{CString, NulError};

use xplm_sys::utilities::*;

/// Possible errors encountered when finding a command
#[derive(Debug,Clone)]
pub enum SearchError {
    /// Indicates that the provided name contains one or more null bytes
    /// Includes the NulError to provide more details
    InvalidName(NulError),
    /// Indicates that no dataref with the specified name was found
    NotFound,
}

impl From<NulError> for SearchError {
    fn from(e: NulError) -> Self {
        SearchError::InvalidName(e)
    }
}
/// Possible errors encountered when creating a command
#[derive(Debug,Clone)]
pub enum CreateError {
    /// Indicates that the provided name contains one or more null bytes
    /// Includes the NulError to provide more details
    InvalidName(NulError),
    /// Indicates that the provided name was empty
    EmptyName,
}

impl From<NulError> for CreateError {
    fn from(e: NulError) -> Self {
        CreateError::InvalidName(e)
    }
}

/// Trait for things that can respond to commands
pub trait CommandCallback {
    /// Called when a command begins execution
    fn command_begin(&mut self);
    /// Called frequently while a command is being executed
    fn command_continue(&mut self);
    /// Called when a command finishes execution
    fn command_end(&mut self);
}

/// Marks a command that this plugin owns
pub struct Owned;

/// Marks a command that is owned by something else
pub struct External;

/// A command
///
/// The type parameter O indicates whether this plugin or something else owns the command.
pub struct Command<O> {
    /// The command
    command: XPLMCommandRef,
    /// Raw pointer to the callback, if this command is owned and has a callback
    callback: Option<*mut CommandCallback>,
    /// The global callback for the associated callback type
    global_callback: XPLMCommandCallback_f,
    /// Phantom storage for the type parameter
    phantom: PhantomData<O>,
}

impl<O> Command<O> {
    /// If this object has a callback, deregisters and deletes it
    fn clear_callback(&mut self) {
        if let (Some(callback), Some(global_callback)) = (self.callback, self.global_callback) {
            unsafe {
                XPLMUnregisterCommandHandler(self.command, Some(global_callback), BEFORE,
                callback as *mut ::libc::c_void);
            }
            let callback_box = unsafe { Box::from_raw(callback) };
            drop(callback_box);
            self.callback = None;
            self.global_callback = None;
        }
    }
}

impl Command<External> {
    /// Finds a command by its name
    pub fn find(name: &str) -> Result<Command<External>, SearchError> {
        let name_c = try!(CString::new(name));
        let command = unsafe { XPLMFindCommand(name_c.as_ptr()) };
        if command.is_null() {
            return Err(SearchError::NotFound);
        }
        Ok(Command {
            command: command,
            callback: None,
            global_callback: None,
            phantom: PhantomData,
        })
    }

    /// Starts executing this command
    pub fn begin(&self) {
        unsafe { XPLMCommandBegin(self.command) };
    }
    /// Finishes executing this command
    pub fn end(&self) {
        unsafe { XPLMCommandEnd(self.command) };
    }
}
/// If handlers are executed before other handlers
const BEFORE: ::libc::c_int = 0;

impl Command<Owned> {
    /// Creates a command
    ///
    /// name: The name of the command
    ///
    /// description: Brief text that describes the command
    pub fn create(name: &str, description: &str) -> Result<Command<Owned>, CreateError> {
        if name.len() < 1 {
            return Err(CreateError::EmptyName);
        }
        let name_c = try!(CString::new(name));
        let description_c = try!(CString::new(description));
        let command = unsafe { XPLMCreateCommand(name_c.as_ptr(), description_c.as_ptr()) };
        Ok(Command {
            command: command,
            callback: None,
            global_callback: None,
            phantom: PhantomData,
        })
    }

    /// Sets the callback that will be called when the command is executed
    pub fn set_callback<C>(&mut self, callback: C) where C: 'static + CommandCallback {
        self.clear_callback();
        let callback_box = Box::new(callback);
        let callback_ptr = Box::into_raw(callback_box);

        unsafe {
            XPLMRegisterCommandHandler(self.command, Some(global_callback::<C>), BEFORE,
                callback_ptr as *mut ::libc::c_void);
        }
        self.callback = Some(callback_ptr);
        self.global_callback = Some(global_callback::<C>);
    }
}

impl<O> Drop for Command<O> {
    fn drop(&mut self) {
        // If this is an owned command, unregister and delete the callback
        self.clear_callback();
    }
}

/// The global callback used for all commands
#[allow(non_upper_case_globals)]
unsafe extern "C" fn global_callback<C>(_: XPLMCommandRef, phase: XPLMCommandPhase,
                                           refcon: *mut ::libc::c_void) -> ::libc::c_int
                                           where C: CommandCallback {
    let callback = refcon as *mut C;
    match phase as u32 {
        xplm_CommandBegin => (*callback).command_begin(),
        xplm_CommandContinue => (*callback).command_continue(),
        xplm_CommandEnd => (*callback).command_end(),
        _ => println!("Unrecognized command phase {}", phase),
    }
    // Allow other things to handle this command
    1
}