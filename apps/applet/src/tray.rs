use std::sync::Mutex;

use crate::AppletMsg;

pub(crate) struct MixCtlTray {
    pub msg_tx: Mutex<std::sync::mpsc::Sender<AppletMsg>>,
}

impl ksni::Tray for MixCtlTray {
    const MENU_ON_ACTIVATE: bool = false;

    fn id(&self) -> String {
        "mixctl-applet".into()
    }

    fn title(&self) -> String {
        "MixCtl".into()
    }

    fn icon_name(&self) -> String {
        "audio-volume-medium".into()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.msg_tx.lock().unwrap().send(AppletMsg::TogglePopup).ok();
    }

    fn menu(&self) -> Vec<ksni::menu::MenuItem<Self>> {
        vec![
            ksni::menu::StandardItem {
                label: "Open Mixer".into(),
                activate: Box::new(|_: &mut Self| {
                    std::process::Command::new("mixctl-ui").spawn().ok();
                }),
                ..Default::default()
            }
            .into(),
            ksni::menu::MenuItem::Separator,
            ksni::menu::StandardItem {
                label: "Quit".into(),
                activate: Box::new(|tray: &mut Self| {
                    tray.msg_tx.lock().unwrap().send(AppletMsg::Quit).ok();
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}
