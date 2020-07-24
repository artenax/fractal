use log::error;

use crate::app::RUNTIME;
use crate::backend::register;
use fractal_api::identifiers::{DeviceId, UserId};
use fractal_api::r0::AccessToken;
use fractal_api::url::Url;
use fractal_api::Session;

use crate::app::App;
use crate::appop::AppOp;

use crate::backend::HandleError;
use crate::cache;
use crate::client::get_matrix_client;

use std::thread;

use crate::passwd::PasswordStorage;

use crate::actions::AppState;

use super::LoginData;

impl AppOp {
    pub fn bk_login(
        &mut self,
        uid: UserId,
        access_token: AccessToken,
        device_id: Box<DeviceId>,
        server_url: Url,
        identity_url: Url,
    ) {
        if self.store_token(uid.clone(), access_token.clone()).is_err() {
            error!("Can't store the token using libsecret");
        }

        let matrix_client =
            get_matrix_client(server_url).expect("Failed to login with the Matrix client");

        self.set_login_data(LoginData {
            session_client: matrix_client.clone(),
            uid: uid.clone(),
            access_token: access_token.clone(),
            device_id: device_id.clone(),
            username: None,
            avatar: None,
            identity_url,
        });

        let _ = RUNTIME
            .handle()
            .block_on(matrix_client.restore_login(Session {
                access_token: access_token.to_string(),
                user_id: uid,
                device_id,
            }));

        self.set_state(AppState::NoRoom);
        self.since = None;
        self.get_username();

        // initial sync, we're shoing some feedback to the user
        self.initial_sync(true);
        self.sync(true, 0);
        self.init_protocols();
    }

    pub fn bk_logout(&mut self) {
        self.set_rooms(vec![], true);
        if cache::get().destroy().is_err() {
            error!("Error removing cache file");
        }

        self.syncing = false;

        self.set_state(AppState::Login);
        self.login_data = None;
    }

    pub fn connect(&mut self, username: String, password: String, server: Url, identity: Url) {
        self.store_pass(
            username.clone(),
            password.clone(),
            server.clone(),
            identity.clone(),
        )
        .unwrap_or_else(|_| {
            // TODO: show an error
            error!("Can't store the password using libsecret");
        });

        thread::spawn(
            move || match register::login(username, password, server.clone()) {
                Ok((uid, tk, dev)) => {
                    APPOP!(bk_login, (uid, tk, dev, server, identity));
                }
                Err(err) => {
                    err.handle_error();
                }
            },
        );
    }

    // TODO: Remove function
    pub fn disconnect(&self) {}

    pub fn logout(&mut self) {
        let login_data = unwrap_or_unit_return!(self.login_data.clone());
        let _ = self.delete_pass("fractal");
        thread::spawn(move || {
            match register::logout(
                login_data.session_client.homeserver().clone(),
                login_data.access_token,
            ) {
                Ok(_) => {
                    APPOP!(bk_logout);
                }
                Err(err) => {
                    err.handle_error();
                }
            }
        });
        self.bk_logout();
        *self.room_back_history.borrow_mut() = vec![];
    }
}
