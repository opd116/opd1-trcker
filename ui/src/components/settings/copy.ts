// The Settings page's words, exactly as the design writes them (TrackerApp.dc.html `extraVals4`, 2012–2038).
import type { Settings, SmurfLevel } from "../../lib/types";

export type ToggleKey = Exclude<keyof Settings, "smurf">;

export const PAGE = { title: "Settings", subtitle: "Changes apply right away and are stored on this PC." } as const;

export const TOGGLES: Record<ToggleKey, { label: string; desc: string }> = {
  highlight: {
    label: "Highlight standout stats",
    desc: "Win%, K/D and HS% well above average turn white. Values well below average get dimmer but stay readable.",
  },
  enemyFirst: {
    label: "Enemy team first",
    desc: "Puts the enemy table on top once the match starts. Agent select always shows your team.",
  },
  savedAlert: {
    label: "Saved player in my match",
    desc: "A Windows notification with your note when someone you saved loads in.",
  },
  sound: { label: "Play a sound with alerts", desc: "A short chime at the Windows notification volume." },
  onTop: {
    label: "Keep on top during matches",
    desc: "Stays above the game while a match is live. Same as the pin in the title bar.",
  },
  boot: { label: "Launch with Windows", desc: "Starts minimized in the tray." },
  tray: { label: "Close to tray", desc: "The × button hides the window instead of quitting." },
};

/** Why "Play a sound" is off limits while the alert itself is off (its tooltip). */
export const SOUND_NEEDS_ALERT = "Turn on “Saved player in my match” first";

export const SMURF = {
  label: "Smurf warnings",
  desc: "Flags accounts whose stats don’t fit their level or rank history.",
  options: [
    { value: "off", label: "Off", desc: "No smurf flags anywhere in the app." },
    {
      value: "cautious",
      label: "Cautious",
      desc: "Flags only when 3 of 4 signals agree: level under 60, headshots 12+ points above the lobby, win rate over 70%, fewer than 40 ranked games. Few false alarms; some smurfs slip through.",
    },
    {
      value: "eager",
      label: "Eager",
      desc: "Flags when any 2 signals agree. Catches more smurfs, but also flags some returning players on new accounts.",
    },
  ] as const satisfies readonly { value: SmurfLevel; label: string; desc: string }[],
};

export type Row = { kind: "toggle"; key: ToggleKey } | { kind: "smurf" };

export const GROUPS: readonly { title: string; desc: string; rows: readonly Row[] }[] = [
  {
    title: "Live match",
    desc: "How the player table looks while you’re in a match.",
    rows: [
      { kind: "toggle", key: "highlight" },
      { kind: "toggle", key: "enemyFirst" },
    ],
  },
  {
    title: "Player signals",
    desc: "Automated guesses about other players. Always labelled as a guess, never as fact.",
    rows: [{ kind: "smurf" }],
  },
  {
    title: "Alerts",
    desc: "Notifications when something in a match needs your attention.",
    rows: [
      { kind: "toggle", key: "savedAlert" },
      { kind: "toggle", key: "sound" },
    ],
  },
  {
    title: "Window and startup",
    desc: "How OPD1 behaves alongside the game.",
    rows: [
      { kind: "toggle", key: "onTop" },
      { kind: "toggle", key: "boot" },
      { kind: "toggle", key: "tray" },
    ],
  },
];

/** The reason a toggle is disabled now, or null: "Play a sound" needs "Saved player in my match". */
export function disabledReason(key: ToggleKey, s: Settings): string | null {
  return key === "sound" && !s.savedAlert ? SOUND_NEEDS_ALERT : null;
}
