import React, { useEffect, useState } from "react";
import { useSelector, useDispatch } from "react-redux";
import { getLastMessagesFromInbox, getLastUnreadMessagesFromInbox } from "../api/index";
import { ShinkaiMessage } from "../models/ShinkaiMessage";
import { IonList, IonItem, IonButton } from "@ionic/react";
import Avatar from "../components/ui/Avatar";
import { cn } from "../theme/lib/utils";
import { IonContentCustom } from "./ui/Layout";
import { calculateMessageHash } from "../utils/shinkai_message_handler";
import { RootState } from "../store";

interface ChatMessagesProps {
  deserializedId: string;
}

const ChatMessages: React.FC<ChatMessagesProps> = ({ deserializedId }) => {
  console.log("Loading ChatMessages.tsx");
  const dispatch = useDispatch();
  const setupDetailsState = useSelector(
    (state: RootState) => state.setupDetails
  );
  const reduxMessages = useSelector(
    (state: RootState) => state.messages.inboxes[deserializedId]
  );

  const [lastKey, setLastKey] = useState<string | undefined>(undefined);
  const [mostRecentKey, setMostRecentKey] = useState<string | undefined>(undefined);
  const [prevMessagesLength, setPrevMessagesLength] = useState(0);
  const [hasMoreMessages, setHasMoreMessages] = useState(true);
  const [messages, setMessages] = useState<ShinkaiMessage[]>([]);

  useEffect(() => {
    console.log("deserializedId:", deserializedId);
    dispatch(
      getLastMessagesFromInbox(deserializedId, 10, lastKey, setupDetailsState)
    );
  }, [dispatch, setupDetailsState]);

  useEffect(() => {
    const interval = setInterval(() => {
      const lastMessage = reduxMessages[reduxMessages.length - 1];
      const hashKey = calculateMessageHash(lastMessage);
      dispatch(
        getLastUnreadMessagesFromInbox(deserializedId, 10, mostRecentKey, setupDetailsState)
      );
    }, 5000); // 2000 milliseconds = 2 seconds
    return () => clearInterval(interval);
  }, [dispatch, deserializedId, mostRecentKey, setupDetailsState, reduxMessages]);

  useEffect(() => {
    if (reduxMessages && reduxMessages.length > 0) {
      // console.log("Redux Messages:", reduxMessages);
      const lastMessage = reduxMessages[reduxMessages.length - 1];
      console.log("Last Message:", lastMessage);
      const timeKey = lastMessage.external_metadata.scheduled_time;
      const hashKey = calculateMessageHash(lastMessage);
      const lastMessageKey = `${timeKey}:::${hashKey}`;
      setLastKey(lastMessageKey);

      const mostRecentMessage = reduxMessages[0];
      const mostRecentTimeKey = mostRecentMessage.external_metadata.scheduled_time;
      const mostRecentHashKey = calculateMessageHash(mostRecentMessage);
      const mostRecentMessageKey = `${mostRecentTimeKey}:::${mostRecentHashKey}`;
      setMostRecentKey(mostRecentMessageKey);

      setMessages(reduxMessages);

      if (reduxMessages.length - prevMessagesLength < 10) {
        setHasMoreMessages(false);
      }
      setPrevMessagesLength(reduxMessages.length);
    }
  }, [reduxMessages]);

  const extractContent = (messageBody: any) => {
    // TODO: extend it so it can be re-used by JobChat or normal Chat
    if (messageBody && "unencrypted" in messageBody) {
      if ("unencrypted" in messageBody.unencrypted.message_data) {
        return JSON.parse(
          messageBody.unencrypted.message_data.unencrypted.message_raw_content
        ).content;
      } else {
        return JSON.parse(
          messageBody.unencrypted.message_data.encrypted.content
        ).content;
      }
    } else if (messageBody?.encrypted) {
      return JSON.parse(messageBody.encrypted.content).content;
    }
    return "";
  };

  return (
    <IonContentCustom>
      <div className="py-10 md:rounded-[1.25rem] bg-white dark:bg-slate-800">
        {hasMoreMessages && (
          <IonButton
            onClick={() =>
              dispatch(
                getLastMessagesFromInbox(
                  deserializedId,
                  10,
                  lastKey,
                  setupDetailsState,
                  true
                )
              )
            }
          >
            Load More
          </IonButton>
        )}
        <IonList class="ion-list-chat p-0 divide-y divide-slate-200 dark:divide-slate-500/50 md:rounded=[1.25rem]  ">
          {messages &&
            messages
              .slice()
              .map((message, index) => {
                const { shinkai_identity, profile, registration_name } =
                  setupDetailsState;

                const localIdentity = `${profile}/device/${registration_name}`;
                // console.log("Message:", message);
                let isLocalMessage = false;
                if (message.body && "unencrypted" in message.body) {
                  isLocalMessage =
                    message.body.unencrypted.internal_metadata
                      .sender_subidentity === localIdentity;
                }

                return (
                  <IonItem
                    key={index}
                    lines="none"
                    className={cn(
                      "ion-item-chat relative w-full shadow",
                      isLocalMessage && "isLocalMessage"
                    )}
                  >
                    <div className="px-2 py-4 flex gap-4 pb-10 w-full">
                      <Avatar
                        className="shrink-0 mr-4"
                        url={
                          isLocalMessage
                            ? "https://ui-avatars.com/api/?name=Me&background=FE6162&color=fff"
                            : "https://ui-avatars.com/api/?name=O&background=363636&color=fff"
                        }
                      />

                      <p>{extractContent(message.body)}</p>
                      {message?.external_metadata?.scheduled_time && (
                        <span className="absolute bottom-[5px] right-5 text-muted text-sm">
                          {new Date(
                            message.external_metadata.scheduled_time
                          ).toLocaleString(undefined, {
                            year: "numeric",
                            month: "long",
                            day: "numeric",
                            hour: "2-digit",
                            minute: "2-digit",
                          })}
                        </span>
                      )}
                    </div>
                  </IonItem>
                );
              })}
        </IonList>
      </div>
    </IonContentCustom>
  );
};

export default ChatMessages;
