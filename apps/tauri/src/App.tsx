import { Routes, Route } from "react-router-dom";
import MainPage from "./pages/MainPage";
import AppletPage from "./pages/AppletPage";
import DspPage from "./pages/DspPage";
import BeacnPage from "./pages/BeacnPage";
import ChannelEditorPage from "./pages/ChannelEditorPage";

export default function App() {
  return (
    <Routes>
      <Route path="/" element={<MainPage />} />
      <Route path="/applet" element={<AppletPage />} />
      <Route path="/dialogs/dsp" element={<DspPage />} />
      <Route path="/dialogs/beacn" element={<BeacnPage />} />
      <Route path="/dialogs/channel-editor" element={<ChannelEditorPage />} />
    </Routes>
  );
}
