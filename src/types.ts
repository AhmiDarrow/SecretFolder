export type ItemKind = "text" | "image" | "binary" | "folder";

export interface VaultStatus {
  initialized: boolean;
  unlocked: boolean;
  itemCount: number;
  idleLockSecs: number;
  hasRecoveryKey: boolean;
}

export interface ItemPreview {
  id: string;
  name: string;
  mime: string;
  size: number;
  kind: ItemKind;
  parentId?: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface ItemDetail {
  id: string;
  name: string;
  mime: string;
  size: number;
  kind: ItemKind;
  parentId?: string | null;
  createdAt: string;
  updatedAt: string;
  text: string | null;
  dataUrl: string | null;
}

export const MAX_FILE_BYTES = 25 * 1024 * 1024;
