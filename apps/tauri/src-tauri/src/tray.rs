use std::sync::Mutex;
use ksni::TrayMethods as _;

pub async fn spawn_tray(tray: MixCtlTray) -> Result<ksni::Handle<MixCtlTray>, ksni::Error> {
    tray.spawn().await
}

pub enum TrayAction {
    ToggleApplet { x: i32, y: i32 },
    ShowMixer,
    Quit,
}

pub struct MixCtlTray {
    pub action_tx: Mutex<tokio::sync::mpsc::UnboundedSender<TrayAction>>,
}

impl ksni::Tray for MixCtlTray {
    const MENU_ON_ACTIVATE: bool = false;

    fn id(&self) -> String {
        "mixctl".into()
    }

    fn title(&self) -> String {
        "MixCtl".into()
    }

    fn icon_name(&self) -> String {
        "audio-card".into()
    }

    fn activate(&mut self, x: i32, y: i32) {
        self.action_tx
            .lock()
            .unwrap()
            .send(TrayAction::ToggleApplet { x, y })
            .ok();
    }

    fn menu(&self) -> Vec<ksni::menu::MenuItem<Self>> {
        vec![
            ksni::menu::StandardItem {
                label: "Show Mixer".into(),
                activate: Box::new(|tray: &mut Self| {
                    tray.action_tx
                        .lock()
                        .unwrap()
                        .send(TrayAction::ShowMixer)
                        .ok();
                }),
                ..Default::default()
            }
            .into(),
            ksni::menu::MenuItem::Separator,
            ksni::menu::StandardItem {
                label: "Quit".into(),
                activate: Box::new(|tray: &mut Self| {
                    tray.action_tx
                        .lock()
                        .unwrap()
                        .send(TrayAction::Quit)
                        .ok();
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}
