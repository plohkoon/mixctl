use crate::State;

pub trait MixCtlApi: Send + Sync + 'static {
    fn ping(&self) -> String;

    fn get_state(&self) -> State;

    fn set_profile(&self, name: String);
}
