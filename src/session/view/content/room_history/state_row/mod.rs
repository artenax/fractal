mod creation;
mod tombstone;

use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{glib, CompositeTemplate};
use log::warn;
use matrix_sdk_ui::timeline::{
    AnyOtherFullStateEventContent, MemberProfileChange, MembershipChange, OtherState,
    RoomMembershipChange, TimelineItemContent,
};
use ruma::{
    events::{room::member::MembershipState, FullStateEventContent},
    UserId,
};

use self::{creation::StateCreation, tombstone::StateTombstone};
use super::ReadReceiptsList;
use crate::{gettext_f, prelude::*, session::model::Event};

mod imp {
    use glib::subclass::InitializingObject;

    use super::*;
    use crate::utils::template_callbacks::TemplateCallbacks;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(
        resource = "/org/gnome/Fractal/ui/session/view/content/room_history/state_row/mod.ui"
    )]
    pub struct StateRow {
        #[template_child]
        pub content: TemplateChild<adw::Bin>,
        #[template_child]
        pub read_receipts: TemplateChild<ReadReceiptsList>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for StateRow {
        const NAME: &'static str = "ContentStateRow";
        type Type = super::StateRow;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
            TemplateCallbacks::bind_template_callbacks(klass);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for StateRow {}
    impl WidgetImpl for StateRow {}
    impl BinImpl for StateRow {}
}

glib::wrapper! {
    pub struct StateRow(ObjectSubclass<imp::StateRow>)
        @extends gtk::Widget, adw::Bin, @implements gtk::Accessible;
}

impl StateRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn content(&self) -> &adw::Bin {
        &self.imp().content
    }

    pub fn set_event(&self, event: &Event) {
        match event.content() {
            TimelineItemContent::MembershipChange(membership_change) => {
                self.update_with_membership_change(&membership_change, &event.sender_id())
            }
            TimelineItemContent::ProfileChange(profile_change) => {
                self.update_with_profile_change(&profile_change, &event.sender().display_name())
            }
            TimelineItemContent::OtherState(other_state) => {
                self.update_with_other_state(event, &other_state)
            }
            _ => unreachable!(),
        }

        self.imp().read_receipts.set_list(event.read_receipts());
    }

    fn update_with_other_state(&self, event: &Event, other_state: &OtherState) {
        let widget = match other_state.content() {
            AnyOtherFullStateEventContent::RoomCreate(content) => {
                WidgetType::Creation(StateCreation::new(content))
            }
            AnyOtherFullStateEventContent::RoomEncryption(_) => {
                WidgetType::Text(gettext("This room is encrypted from this point on."))
            }
            AnyOtherFullStateEventContent::RoomThirdPartyInvite(content) => {
                let display_name = match content {
                    FullStateEventContent::Original { content, .. } => {
                        match &content.display_name {
                            s if s.is_empty() => other_state.state_key(),
                            s => s,
                        }
                    }
                    FullStateEventContent::Redacted(_) => other_state.state_key(),
                };
                WidgetType::Text(gettext_f(
                    // Translators: Do NOT translate the content between '{' and '}', this is a
                    // variable name.
                    "{user} was invited to this room.",
                    &[("user", display_name)],
                ))
            }
            AnyOtherFullStateEventContent::RoomTombstone(_) => {
                WidgetType::Tombstone(StateTombstone::new(&event.room()))
            }
            _ => {
                warn!(
                    "Unsupported state event: {}",
                    other_state.content().event_type()
                );
                WidgetType::Text(gettext("An unsupported state event was received."))
            }
        };

        let content = self.content();
        match widget {
            WidgetType::Text(message) => {
                if let Some(Ok(child)) = content.child().map(|w| w.downcast::<gtk::Label>()) {
                    child.set_text(&message);
                } else {
                    content.set_child(Some(&text(message)));
                };
            }
            WidgetType::Creation(widget) => content.set_child(Some(&widget)),
            WidgetType::Tombstone(widget) => content.set_child(Some(&widget)),
        }
    }

    fn update_with_membership_change(
        &self,
        membership_change: &RoomMembershipChange,
        sender: &UserId,
    ) {
        let display_name = match membership_change.content() {
            FullStateEventContent::Original { content, .. } => content
                .displayname
                .clone()
                .unwrap_or_else(|| membership_change.user_id().to_string()),
            FullStateEventContent::Redacted(_) => membership_change.user_id().to_string(),
        };

        // Fallback to showing the membership when we don't know / don't want to show
        // the change.
        let supported_membership_change =
            match membership_change.change().unwrap_or(MembershipChange::None) {
                MembershipChange::Joined => MembershipChange::Joined,
                MembershipChange::Left => MembershipChange::Left,
                MembershipChange::Banned => MembershipChange::Banned,
                MembershipChange::Unbanned => MembershipChange::Unbanned,
                MembershipChange::Kicked => MembershipChange::Kicked,
                MembershipChange::Invited => MembershipChange::Invited,
                MembershipChange::KickedAndBanned => MembershipChange::KickedAndBanned,
                MembershipChange::InvitationAccepted => MembershipChange::InvitationAccepted,
                MembershipChange::InvitationRejected => MembershipChange::InvitationRejected,
                MembershipChange::InvitationRevoked => MembershipChange::InvitationRevoked,
                MembershipChange::Knocked => MembershipChange::Knocked,
                MembershipChange::KnockAccepted => MembershipChange::KnockAccepted,
                MembershipChange::KnockRetracted => MembershipChange::KnockRetracted,
                MembershipChange::KnockDenied => MembershipChange::KnockDenied,
                _ => {
                    let membership = match membership_change.content() {
                        FullStateEventContent::Original { content, .. } => &content.membership,
                        FullStateEventContent::Redacted(content) => &content.membership,
                    };

                    match membership {
                        MembershipState::Ban => MembershipChange::Banned,
                        MembershipState::Invite => MembershipChange::Invited,
                        MembershipState::Join => MembershipChange::Joined,
                        MembershipState::Knock => MembershipChange::Knocked,
                        MembershipState::Leave => {
                            if membership_change.user_id() == sender {
                                MembershipChange::Left
                            } else {
                                MembershipChange::Kicked
                            }
                        }
                        _ => MembershipChange::NotImplemented,
                    }
                }
            };

        let message = match supported_membership_change {
            MembershipChange::Joined => {
                // Translators: Do NOT translate the content between '{' and '}', this
                // is a variable name.
                gettext_f("{user} joined this room.", &[("user", &display_name)])
            }
            MembershipChange::Left => {
                // Translators: Do NOT translate the content between '{' and '}',
                // this is a variable name.
                gettext_f("{user} left the room.", &[("user", &display_name)])
            }
            MembershipChange::Banned => gettext_f(
                // Translators: Do NOT translate the content between
                // '{' and '}', this is a variable name.
                "{user} was banned.",
                &[("user", &display_name)],
            ),
            MembershipChange::Unbanned => gettext_f(
                // Translators: Do NOT translate the content between
                // '{' and '}', this is a variable name.
                "{user} was unbanned.",
                &[("user", &display_name)],
            ),
            MembershipChange::Kicked => gettext_f(
                // Translators: Do NOT translate the content between '{' and
                // '}', this is a variable name.
                "{user} was kicked out of the room.",
                &[("user", &display_name)],
            ),
            MembershipChange::Invited | MembershipChange::KnockAccepted => gettext_f(
                // Translators: Do NOT translate the content between '{' and '}', this is
                // a variable name.
                "{user} was invited to this room.",
                &[("user", &display_name)],
            ),
            MembershipChange::KickedAndBanned => gettext_f(
                // Translators: Do NOT translate the content between '{' and '}', this is
                // a variable name.
                "{user} was kicked out of the room and banned.",
                &[("user", &display_name)],
            ),
            MembershipChange::InvitationAccepted => gettext_f(
                // Translators: Do NOT translate the content between
                // '{' and '}', this is a variable name.
                "{user} accepted the invite.",
                &[("user", &display_name)],
            ),
            MembershipChange::InvitationRejected => gettext_f(
                // Translators: Do NOT translate the content between
                // '{' and '}', this is a variable name.
                "{user} rejected the invite.",
                &[("user", &display_name)],
            ),
            MembershipChange::InvitationRevoked => gettext_f(
                // Translators: Do NOT translate the content between
                // '{' and '}', this is a variable name.
                "The invitation for {user} has been revoked.",
                &[("user", &display_name)],
            ),
            MembershipChange::Knocked =>
            // TODO: Add button to invite the user.
            {
                gettext_f(
                    // Translators: Do NOT translate the content between '{' and '}', this
                    // is a variable name.
                    "{user} requested to be invited to this room.",
                    &[("user", &display_name)],
                )
            }
            MembershipChange::KnockRetracted => gettext_f(
                // Translators: Do NOT translate the content between
                // '{' and '}', this is a variable name.
                "{user} retracted their request to be invited to this room.",
                &[("user", &display_name)],
            ),
            MembershipChange::KnockDenied => gettext_f(
                // Translators: Do NOT translate the content between
                // '{' and '}', this is a variable name.
                "{user}’s request to be invited to this room was denied.",
                &[("user", &display_name)],
            ),
            _ => {
                warn!(
                    "Unsupported membership change event: {:?}",
                    membership_change.content()
                );
                gettext("An unsupported room member event was received.")
            }
        };

        let content = self.content();
        if let Some(Ok(child)) = content.child().map(|w| w.downcast::<gtk::Label>()) {
            child.set_text(&message);
        } else {
            content.set_child(Some(&text(message)));
        };
    }

    fn update_with_profile_change(&self, profile_change: &MemberProfileChange, display_name: &str) {
        let message = if let Some(displayname) = profile_change.displayname_change() {
            if let Some(prev_name) = &displayname.old {
                if displayname.new.is_none() {
                    gettext_f(
                        // Translators: Do NOT translate the content between
                        // '{' and '}', this is a variable name.
                        "{previous_user_name} removed their display name.",
                        &[("previous_user_name", prev_name)],
                    )
                } else {
                    gettext_f(
                        // Translators: Do NOT translate the content between
                        // '{' and '}', this is a variable name.
                        "{previous_user_name} changed their display name to {new_user_name}.",
                        &[
                            ("previous_user_name", prev_name),
                            ("new_user_name", display_name),
                        ],
                    )
                }
            } else {
                gettext_f(
                    // Translators: Do NOT translate the content between
                    // '{' and '}', this is a variable name.
                    "{user_id} set their display name to {new_user_name}.",
                    &[
                        ("user_id", profile_change.user_id().as_ref()),
                        ("new_user_name", display_name),
                    ],
                )
            }
        } else if let Some(avatar_url) = profile_change.avatar_url_change() {
            if avatar_url.old.is_none() {
                gettext_f(
                    // Translators: Do NOT translate the content between
                    // '{' and '}', this is a variable name.
                    "{user} set their avatar.",
                    &[("user", display_name)],
                )
            } else if avatar_url.new.is_none() {
                gettext_f(
                    // Translators: Do NOT translate the content between
                    // '{' and '}', this is a variable name.
                    "{user} removed their avatar.",
                    &[("user", display_name)],
                )
            } else {
                gettext_f(
                    // Translators: Do NOT translate the content between
                    // '{' and '}', this is a variable name.
                    "{user} changed their avatar.",
                    &[("user", display_name)],
                )
            }
        } else {
            // We don't know what changed so fall back to the membership.
            // Translators: Do NOT translate the content between '{' and '}', this
            // is a variable name.
            gettext_f("{user} joined this room.", &[("user", display_name)])
        };

        let content = self.content();
        if let Some(Ok(child)) = content.child().map(|w| w.downcast::<gtk::Label>()) {
            child.set_text(&message);
        } else {
            content.set_child(Some(&text(message)));
        };
    }
}

enum WidgetType {
    Text(String),
    Creation(StateCreation),
    Tombstone(StateTombstone),
}

fn text(label: String) -> gtk::Label {
    let child = gtk::Label::new(Some(&label));
    child.set_css_classes(&["event-content", "dim-label"]);
    child.set_wrap(true);
    child.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    child.set_xalign(0.0);
    child
}
