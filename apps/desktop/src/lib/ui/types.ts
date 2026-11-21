/** Shared prop types of the design-system components (`$lib/ui`). */
import type { IconNode } from 'lucide';

export type Tone = 'neutral' | 'accent' | 'success' | 'warning' | 'danger';
export type Size = 'sm' | 'md' | 'lg';

/** One choice of `Select`, `SegmentedControl` or `RadioGroup`. */
export interface SelectOption<T> {
  value: T;
  /** Translated text. */
  label: string;
  description?: string;
  icon?: IconNode;
  disabled?: boolean;
}

/** One entry of a `Menu`: an action or a separator. */
export type MenuItem =
  | {
      id: string;
      label: string;
      icon?: IconNode;
      /** Shortcut hint shown on the right (`['Mod', 'K']`). */
      keys?: string[];
      danger?: boolean;
      disabled?: boolean;
      onselect: () => void;
    }
  | { id: string; separator: true };

/** One tab of `Tabs`. */
export interface TabItem {
  id: string;
  label: string;
  icon?: IconNode;
  /** Count shown after the label (hidden when 0 or absent). */
  badge?: number | null;
  disabled?: boolean;
}

/** A column of `Table`. */
export interface TableColumn<T> {
  id: string;
  header: string;
  /** CSS grid track (`'1fr'`, `'120px'`, `'minmax(80px, 1fr)'`). Default `1fr`. */
  width?: string;
  align?: 'start' | 'end' | 'center';
  /** Numbers (counts, dates, durations): mono, tabular figures. */
  numeric?: boolean;
  /** Plain-text cell (when no `cell` snippet is given to the table). */
  value?: (row: T) => string | number | null;
}

/** One step of the top bar's trail (`PageChrome.setTrail`). With `onclick` it links back. */
export interface Crumb {
  label: string;
  onclick?: () => void;
}
