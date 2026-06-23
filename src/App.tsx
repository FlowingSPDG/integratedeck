import { DeckProvider, useDeck } from "./deck/DeckContext";
import { ActionSidebar } from "./components/ActionSidebar";
import { ConfigPanel } from "./components/ConfigPanel";
import { DeviceGrid } from "./components/DeviceGrid";
import { DragGhostOverlay } from "./components/DragGhostOverlay";
import { PageNav } from "./components/PageNav";
import { SlotContextMenu } from "./components/SlotContextMenu";
import { StartupConflictBanner } from "./components/StartupConflictBanner";
import { Toolbar } from "./components/Toolbar";

function DeckWorkspace() {
  const { deviceLabel } = useDeck();

  return (
    <div className="app-shell">
      <Toolbar />
      <StartupConflictBanner />
      <div className="workspace">
        <div className="center-panel">
          <div className="device-frame">
            <div className="device-shell">
              <div className="device-label">{deviceLabel}</div>
              <DeviceGrid />
            </div>
            <PageNav />
          </div>
          <ConfigPanel />
        </div>
        <ActionSidebar />
      </div>
      <DragGhostOverlay />
      <SlotContextMenu />
    </div>
  );
}

export default function App() {
  return (
    <DeckProvider>
      <DeckWorkspace />
    </DeckProvider>
  );
}
