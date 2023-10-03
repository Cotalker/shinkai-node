import { useEffect } from "react";
import { Navigate, Route, Routes } from "react-router-dom";

import { ApiConfig } from "@shinkai_network/shinkai-message-ts/api";
import { invoke } from "@tauri-apps/api/tauri";

import ChatConversation from "../pages/chat/chat-conversation";
import EmptyMessage from "../pages/chat/empty-message";
import ChatLayout from "../pages/chat/layout";
import CreateAgentPage from "../pages/create-agent";
import CreateChatPage from "../pages/create-chat";
import CreateJobPage from "../pages/create-job";
import MainLayout from "../pages/layout/main-layout";
import OnboardingPage from "../pages/onboarding";
import SettingsPage from "../pages/settings";
import { useAuth } from "../store/auth-context";
import {
  ADD_AGENT_PATH,
  CREATE_CHAT_PATH,
  CREATE_JOB_PATH,
  ONBOARDING_PATH,
  SETTINGS_PATH,
} from "./name";

const ProtectedRoute = ({ children }: { children: React.ReactNode }) => {
  const { setupData } = useAuth();

  useEffect(() => {
    ApiConfig.getInstance().setEndpoint(setupData?.node_address ?? "");
  }, [setupData?.node_address]);

  if (!setupData) {
    return <Navigate to={ONBOARDING_PATH} replace />;
  }
  return children;
};

const AppRoutes = () => {
  useEffect(() => {
    console.log("Registering hotkey");
    // Register the global shortcut
    // register("Alt+Shift+Enter", async () => {
    //   console.log("Hotkey activated");
    // });

    // Check if setup data is valid
    (invoke("validate_setup_data") as Promise<boolean>)
      .then((isValid: boolean) => {
        console.log("is already", isValid);
      })
      .catch((error: string) => {
        console.error("Failed to validate setup data:", error);
      });
  }, []);

  return (
    <Routes>
      <Route element={<MainLayout />}>
        <Route element={<OnboardingPage />} path={ONBOARDING_PATH} />
        <Route
          element={
            <ProtectedRoute>
              <ChatLayout />
            </ProtectedRoute>
          }
          path="inboxes/*"
        >
          <Route element={<EmptyMessage />} index />
          <Route element={<ChatConversation />} path=":inboxId" />
        </Route>
        <Route
          element={
            <ProtectedRoute>
              <CreateAgentPage />
            </ProtectedRoute>
          }
          path={ADD_AGENT_PATH}
        />
        <Route
          element={
            <ProtectedRoute>
              <CreateChatPage />
            </ProtectedRoute>
          }
          path={CREATE_CHAT_PATH}
        />
        <Route
          element={
            <ProtectedRoute>
              <CreateJobPage />
            </ProtectedRoute>
          }
          path={CREATE_JOB_PATH}
        />
        <Route
          element={
            <ProtectedRoute>
              <SettingsPage />
            </ProtectedRoute>
          }
          path={SETTINGS_PATH}
        />
      </Route>
      <Route element={<Navigate to={"inboxes/"} replace />} path="/" />
    </Routes>
  );
};
export default AppRoutes;
