use std::cell::{Cell, RefCell};

use gpui_kit::{Pixels, Point, Window};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSControlStateValueOff, NSControlStateValueOn, NSEventModifierFlags, NSImage, NSMenu,
    NSMenuItem, NSSearchField, NSView,
};
use objc2_foundation::{NSArray, NSPoint, NSRect, NSSize, NSString};

use super::{Picker, PickerChoice, PickerRow};
use crate::appkit::gpui_view;

const NO_CHOICE: isize = -1;
const CREATE_TAG: isize = -2;
const FOOTER_BASE: isize = 1 << 16;
const FIELD_W: f64 = 260.0;
const FIELD_H: f64 = 28.0;
const FIELD_INSET_X: f64 = 10.0;
const FIELD_INSET_Y: f64 = 4.0;
const COMMON_MODES: &str = "kCFRunLoopCommonModes";
const TEXT_DID_CHANGE: &str = "NSControlTextDidChangeNotification";

struct PickerIvars {
    chosen: Cell<isize>,
    query: RefCell<String>,
    rows: RefCell<Vec<(Retained<NSMenuItem>, String)>>,
    create: RefCell<Option<(Retained<NSMenuItem>, &'static str)>>,
    field: RefCell<Option<Retained<NSSearchField>>>,
    menu: RefCell<Option<Retained<NSMenu>>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "KradTabPickerTarget"]
    #[ivars = PickerIvars]
    struct PickerTarget;

    impl PickerTarget {
        #[unsafe(method(choose:))]
        fn choose(&self, sender: &NSMenuItem) {
            self.ivars().chosen.set(sender.tag());
        }

        #[unsafe(method(submit:))]
        fn submit(&self, _sender: &AnyObject) {
            let first = self
                .ivars()
                .rows
                .borrow()
                .iter()
                .find(|(item, _)| !item.isHidden())
                .map(|(item, _)| item.tag());
            let creates = self.ivars().create.borrow().is_some()
                && !self.ivars().query.borrow().trim().is_empty();
            match first {
                Some(tag) => self.ivars().chosen.set(tag),
                None if creates => self.ivars().chosen.set(CREATE_TAG),
                None => return,
            }
            if let Some(menu) = self.ivars().menu.borrow().as_ref() {
                menu.cancelTracking();
            }
        }

        #[unsafe(method(textChanged:))]
        fn text_changed(&self, _notification: &AnyObject) {
            let typed = self
                .ivars()
                .field
                .borrow()
                .as_ref()
                .map_or_else(String::new, |field| field.stringValue().to_string());
            self.filter(&typed);
        }

        #[unsafe(method(focusField))]
        fn focus_field(&self) {
            let Some(field) = self.ivars().field.borrow().clone() else {
                return;
            };
            if let Some(window) = field.window() {
                window.makeFirstResponder(Some(&field));
            }
        }
    }
);

impl PickerTarget {
    fn new(main_thread: MainThreadMarker) -> Retained<Self> {
        let this = main_thread.alloc::<Self>().set_ivars(PickerIvars {
            chosen: Cell::new(NO_CHOICE),
            query: RefCell::new(String::new()),
            rows: RefCell::new(Vec::new()),
            create: RefCell::new(None),
            field: RefCell::new(None),
            menu: RefCell::new(None),
        });
        // SAFETY: `NSObject`'s `init` is valid for a freshly allocated subclass instance.
        unsafe { msg_send![super(this), init] }
    }

    fn filter(&self, typed: &str) {
        let query = typed.trim().to_lowercase();
        let mut exact = false;
        for (item, label) in self.ivars().rows.borrow().iter() {
            let lower = label.to_lowercase();
            exact |= lower == query;
            item.setHidden(!query.is_empty() && !lower.contains(&query));
        }
        if let Some((item, verb)) = self.ivars().create.borrow().as_ref() {
            let shown = !query.is_empty() && !exact;
            item.setHidden(!shown);
            item.setTitle(&NSString::from_str(&format!(
                "{verb} \u{201c}{}\u{201d}",
                typed.trim()
            )));
        }
        typed.clone_into(&mut self.ivars().query.borrow_mut());
    }
}

fn menu_row(
    row: &PickerRow,
    tag: isize,
    target: &PickerTarget,
    main_thread: MainThreadMarker,
) -> Retained<NSMenuItem> {
    let key = row.shortcut.map_or_else(String::new, String::from);
    // SAFETY: the item is created with a selector the target implements.
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            main_thread.alloc(),
            &NSString::from_str(&row.label),
            Some(sel!(choose:)),
            &NSString::from_str(&key),
        )
    };
    item.setKeyEquivalentModifierMask(NSEventModifierFlags::Command);
    item.setTag(tag);
    // SAFETY: the target outlives the menu's synchronous tracking loop.
    unsafe { item.setTarget(Some(target)) };
    item.setState(if row.checked {
        NSControlStateValueOn
    } else {
        NSControlStateValueOff
    });
    if let Some(symbol) = row.symbol {
        let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str(symbol),
            None,
        );
        item.setImage(image.as_deref());
    }
    item
}

fn search_item(
    placeholder: &str,
    target: &PickerTarget,
    main_thread: MainThreadMarker,
) -> Retained<NSMenuItem> {
    let frame = NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(FIELD_W, FIELD_H + 2.0 * FIELD_INSET_Y),
    );
    let holder = NSView::initWithFrame(main_thread.alloc(), frame);
    let field_frame = NSRect::new(
        NSPoint::new(FIELD_INSET_X, FIELD_INSET_Y),
        NSSize::new(FIELD_W - 2.0 * FIELD_INSET_X, FIELD_H),
    );
    let field = NSSearchField::initWithFrame(main_thread.alloc(), field_frame);
    field.setPlaceholderString(Some(&NSString::from_str(placeholder)));
    field.setSendsWholeSearchString(true);
    // SAFETY: the target implements `submit:` and outlives the tracking loop.
    unsafe {
        field.setTarget(Some(target));
        field.setAction(Some(sel!(submit:)));
    }
    holder.addSubview(&field);
    let center: Retained<AnyObject> =
        // SAFETY: `NSNotificationCenter.defaultCenter` always returns the shared center.
        unsafe { msg_send![objc2::class!(NSNotificationCenter), defaultCenter] };
    let name = NSString::from_str(TEXT_DID_CHANGE);
    // SAFETY: the observer implements `textChanged:` and is removed below once tracking ends.
    unsafe {
        let () = msg_send![&center, addObserver: target, selector: sel!(textChanged:), name: &*name, object: &*field];
    }
    *target.ivars().field.borrow_mut() = Some(field);
    let item = NSMenuItem::new(main_thread);
    item.setView(Some(&holder));
    item
}

fn build_menu(
    picker: &Picker,
    target: &PickerTarget,
    main_thread: MainThreadMarker,
) -> Retained<NSMenu> {
    let menu = NSMenu::new(main_thread);
    menu.setAutoenablesItems(false);
    menu.addItem(&search_item(picker.placeholder, target, main_thread));
    let mut rows = Vec::new();
    for (index, row) in picker.rows.iter().enumerate() {
        let item = menu_row(
            row,
            isize::try_from(index).unwrap_or(NO_CHOICE),
            target,
            main_thread,
        );
        menu.addItem(&item);
        rows.push((item, row.label.clone()));
    }
    *target.ivars().rows.borrow_mut() = rows;
    if let Some(verb) = picker.create_label {
        let create = menu_row(
            &PickerRow {
                label: verb.to_owned(),
                checked: false,
                shortcut: None,
                symbol: Some("plus"),
            },
            CREATE_TAG,
            target,
            main_thread,
        );
        create.setHidden(true);
        menu.addItem(&create);
        *target.ivars().create.borrow_mut() = Some((create, verb));
    }
    for (index, row) in picker.footer.iter().enumerate() {
        if index == 0 {
            menu.addItem(&NSMenuItem::separatorItem(main_thread));
        }
        let tag = FOOTER_BASE + isize::try_from(index).unwrap_or_default();
        menu.addItem(&menu_row(row, tag, target, main_thread));
    }
    menu
}

pub(crate) struct PickerHost(Retained<NSView>);

pub(crate) fn picker_host(window: &Window) -> Option<PickerHost> {
    gpui_view(window).map(PickerHost)
}

pub(crate) fn show_picker(
    picker: &Picker,
    position: Point<Pixels>,
    host: &PickerHost,
) -> Option<PickerChoice> {
    let main_thread = MainThreadMarker::new()?;
    let view = &host.0;
    let target = PickerTarget::new(main_thread);
    let menu = build_menu(picker, &target, main_thread);
    *target.ivars().menu.borrow_mut() = Some(menu.clone());
    let modes = NSArray::from_retained_slice(&[NSString::from_str(COMMON_MODES)]);
    // SAFETY: `focusField` takes no argument and the target lives through the tracking loop.
    unsafe {
        let () = msg_send![&*target, performSelector: sel!(focusField), withObject: Option::<&AnyObject>::None, afterDelay: 0.0_f64, inModes: &*modes];
    }
    let height = view.bounds().size.height;
    let location = NSPoint::new(
        f64::from(position.x.as_f32()),
        height - f64::from(position.y.as_f32()),
    );
    menu.popUpMenuPositioningItem_atLocation_inView(None, location, Some(view));
    let center: Retained<AnyObject> =
        // SAFETY: `NSNotificationCenter.defaultCenter` always returns the shared center.
        unsafe { msg_send![objc2::class!(NSNotificationCenter), defaultCenter] };
    // SAFETY: removing an observer that was added above.
    let () = unsafe { msg_send![&center, removeObserver: &*target] };
    let query = target.ivars().query.borrow().trim().to_owned();
    match target.ivars().chosen.get() {
        NO_CHOICE => None,
        CREATE_TAG => Some(PickerChoice::Create(query)),
        tag if tag >= FOOTER_BASE => usize::try_from(tag - FOOTER_BASE)
            .ok()
            .map(PickerChoice::Footer),
        tag => usize::try_from(tag).ok().map(PickerChoice::Row),
    }
}
