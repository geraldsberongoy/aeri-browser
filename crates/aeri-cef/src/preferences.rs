//! Chromium preferences of a profile (a CEF request context), set from
//! Aeri's own settings. Nothing here decides what a setting means; later
//! waves build a [`PrefSet`] and give it to
//! [`ProfileContext::apply_preferences`](crate::ProfileContext::apply_preferences).
//!
//! Every call goes through `cef_preference_manager_t::set_preference` and
//! must run on CEF's UI thread, the main thread (`apply_preferences` posts
//! itself there when called elsewhere). Chromium rejects a name it does not
//! know or a value of the wrong type; that is reported, never fatal.
//!
//! Preference names the settings waves use:
//!
//! | Setting (Aeri prefs)                      | Chromium preference                                  | Value  |
//! |-------------------------------------------|------------------------------------------------------|--------|
//! | `general.downloads_ask`                   | `download.prompt_for_download`                       | bool   |
//! | `general.downloads_dir`                   | `download.default_directory`                         | string |
//! | `privacy.notifications_ask`               | `profile.default_content_setting_values.notifications` | int: 1 allow, 2 block, 3 ask |
//! | `browsing.page_zoom_percent`              | `partition.default_zoom_level` (dict, key `""`)      | double, `log(percent/100) / log(1.2)` |
//! | `privacy.prevent_cross_site_tracking`     | `profile.cookie_controls_mode`                       | int: 0 allow all, 1 block third-party, 2 incognito only |
//! | `privacy.gpc`                             | `enable_do_not_track`                                | bool   |
//! | spellcheck settings                       | `browser.enable_spellchecking`                       | bool   |
//! | `history.saving_disabled` (always on)     | `history.saving_disabled`                            | bool   |
//!
//! Policy that is not a preference (autoplay, features) is a command-line
//! switch: see [`add_switch`](crate::add_switch).

use cef::rc::Rc as _;
use cef::{
    CefString, ImplDictionaryValue, ImplListValue, ImplPreferenceManager, ImplTask, ImplValue,
    RequestContext, Task, WrapTask, dictionary_value_create, list_value_create, value_create,
    wrap_task,
};

/// A preference's value.
#[derive(Clone, Debug, PartialEq)]
pub enum PrefValue {
    Bool(bool),
    Int(i32),
    Double(f64),
    String(String),
    List(Vec<PrefValue>),
    /// Key order is kept.
    Dict(Vec<(String, PrefValue)>),
}

/// Preferences to set together, in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PrefSet(pub Vec<(String, PrefValue)>);

impl PrefSet {
    pub fn with(mut self, name: &str, value: PrefValue) -> Self {
        self.0.push((name.to_owned(), value));
        self
    }
}

fn build(value: &PrefValue) -> Result<cef::Value, String> {
    let out = value_create().ok_or("no CEF value")?;
    match value {
        PrefValue::Bool(v) => out.set_bool((*v).into()),
        PrefValue::Int(v) => out.set_int(*v),
        PrefValue::Double(v) => out.set_double(*v),
        PrefValue::String(v) => out.set_string(Some(&CefString::from(v.as_str()))),
        PrefValue::List(items) => {
            let mut list = list_value_create().ok_or("no CEF list")?;
            for (index, item) in items.iter().enumerate() {
                list.set_value(index, Some(&mut build(item)?));
            }
            out.set_list(Some(&mut list))
        }
        PrefValue::Dict(entries) => {
            let mut dict = dictionary_value_create().ok_or("no CEF dictionary")?;
            for (key, item) in entries {
                dict.set_value(
                    Some(&CefString::from(key.as_str())),
                    Some(&mut build(item)?),
                );
            }
            out.set_dictionary(Some(&mut dict))
        }
    };
    Ok(out)
}

/// Sets a preference of the profile. cef-rs's `set_preference` returned
/// false with an empty error string and with none (Observed), while the
/// same call made directly with a zeroed error string succeeds, so this
/// calls CEF directly. UI thread, after the context is initialized.
pub(crate) fn set(context: &RequestContext, name: &str, value: &PrefValue) -> Result<(), String> {
    let cef_value = build(value)?;
    let name = CefString::from(name);
    let manager = ImplPreferenceManager::get_raw(context);
    // SAFETY: `manager` is the live context's preference manager and is
    // used only during this call. CEF takes the value's reference added
    // here, and `error` is a zeroed string CEF may fill; it is cleared
    // before returning.
    unsafe {
        let set = (*manager).set_preference.ok_or("no set_preference")?;
        let mut error: cef::sys::cef_string_t = std::mem::zeroed();
        cef_value.add_ref();
        let done = set(
            manager,
            (&name).into(),
            ImplValue::get_raw(&cef_value),
            &mut error,
        );
        let reason = if error.str_.is_null() {
            String::new()
        } else {
            String::from_utf16_lossy(std::slice::from_raw_parts(error.str_, error.length))
        };
        cef::sys::cef_string_utf16_clear(&mut error);
        if done == 0 { Err(reason) } else { Ok(()) }
    }
}

/// Sets each preference, logging the ones Chromium refuses; returns how many
/// were refused. UI thread.
pub(crate) fn apply(context: &RequestContext, set_of: &PrefSet) -> usize {
    let mut refused = 0;
    for (name, value) in &set_of.0 {
        if let Err(error) = set(context, name, value) {
            refused += 1;
            tracing::warn!(name, error, "Chromium refused a preference");
        }
    }
    refused
}

/// [`apply`] on the UI thread: now when already there, else posted.
pub(crate) fn apply_on_ui(context: RequestContext, set_of: PrefSet) {
    if cef::currently_on(cef::ThreadId::UI) != 0 {
        apply(&context, &set_of);
        return;
    }
    let mut task = ApplyPreferences::new(context, set_of);
    if cef::post_task(cef::ThreadId::UI, Some(&mut task)) == 0 {
        tracing::warn!("could not post preferences to the UI thread");
    }
}

wrap_task! {
    struct ApplyPreferences {
        context: RequestContext,
        set_of: PrefSet,
    }

    impl Task {
        fn execute(&self) {
            apply(&self.context, &self.set_of);
        }
    }
}
