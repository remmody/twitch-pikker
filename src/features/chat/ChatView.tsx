import { memo, useCallback, useLayoutEffect, useRef, useState } from "react";
import { ArrowDown, Trash2 } from "lucide-react";
import { useApp, type LogMessage } from "../../stores/app";
import { useUi } from "../../stores/ui";
import { userColor } from "../../lib/format";
import { Button } from "../../components/ui";

const STICK_THRESHOLD_PX = 48;

const Line = memo(function Line({ m, showTime }: { m: LogMessage; showTime: boolean }) {
  return (
    <li className="break-words px-1 py-1 text-[15px] leading-snug">
      {showTime && <span className="mr-2 text-xs tabular-nums text-muted">{m.time}</span>}
      <span className="font-semibold" style={{ color: userColor(m.username) }}>{m.username}</span>
      <span className="text-muted">: </span>
      <span className={m.is_action ? "italic" : undefined} style={m.is_action ? { color: userColor(m.username) } : undefined}>{m.text}</span>
    </li>
  );
});

export function ChatView() {
  const messages = useApp((s) => s.messages);
  const clear = useApp((s) => s.clearMessages);
  const status = useApp((s) => s.status);
  const showTime = useUi((s) => s.showTimestamps);
  const setUi = useUi((s) => s.set);
  const box = useRef<HTMLDivElement>(null);
  const [live, setLive] = useState(true);
  const liveRef = useRef(true);

  const toBottom = useCallback(() => {
    const el = box.current;
    if (el) el.scrollTop = el.scrollHeight;
    liveRef.current = true;
    setLive(true);
  }, []);

  const onScroll = () => {
    const el = box.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < STICK_THRESHOLD_PX;
    if (atBottom !== liveRef.current) {
      liveRef.current = atBottom;
      setLive(atBottom);
    }
  };

  useLayoutEffect(() => {
    if (liveRef.current && box.current) box.current.scrollTop = box.current.scrollHeight;
  }, [messages]);

  return (
    <div className="relative flex h-full min-h-0 flex-col">
      <div className="flex items-center justify-between gap-3 px-6 py-3">
        <label className="flex cursor-pointer items-center gap-2 text-sm text-muted">
          <input type="checkbox" checked={showTime} onChange={(e) => setUi({ showTimestamps: e.target.checked })} />
          Show timestamps
        </label>
        <Button variant="ghost" onClick={clear} icon={<Trash2 className="h-4 w-4" />} disabled={messages.length === 0}>Clear log</Button>
      </div>

      <div ref={box} onScroll={onScroll} className="min-h-0 flex-1 overflow-y-auto px-6 pb-6" tabIndex={0} aria-label="Chat messages">
        {messages.length === 0 ? (
          <div className="grid h-full place-items-center text-center text-muted">
            <div>
              <p className="text-lg font-medium text-fg">{status.state === "connected" ? "Waiting for messages…" : "Not connected"}</p>
              <p className="mt-1 text-sm">{status.state === "connected" ? "New chat messages will appear here." : "Enter a channel name above and press Connect."}</p>
            </div>
          </div>
        ) : (
          <ul className="space-y-0.5">
            {messages.map((m) => <Line key={m.id} m={m} showTime={showTime} />)}
          </ul>
        )}
      </div>

      {!live && (
        <Button variant="primary" onClick={toBottom} icon={<ArrowDown className="h-4 w-4" />} className="absolute bottom-6 left-1/2 -translate-x-1/2 shadow-card">
          Resume live chat
        </Button>
      )}
    </div>
  );
}
