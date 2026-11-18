import { Icon } from "../primitives/Icon";

export function ChatStart({ resume, onOpen }: { resume: boolean; onOpen: () => void }) {
  return (
    <div className="empty chat-start">
      <Icon name="bubble-chat" />
      <div className="chat-start-text">
        <strong>This chat has not started on its runtime yet.</strong>
        <span className="detail">{resume ? "Open it here to resume where the agent left off." : "Open it here to start the agent in this tab."}</span>
      </div>
      <button className="button primary" onClick={onOpen}>
        {resume ? "Resume chat" : "Open chat"}
      </button>
    </div>
  );
}
