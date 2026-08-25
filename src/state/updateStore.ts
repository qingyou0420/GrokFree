import { create } from "zustand";
import type { CloudUpdateInfo, UpdateProgress } from "../lib/types";

type UpdateState = {
  cloudUpdate: CloudUpdateInfo | null;
  updateBusy: boolean;
  updateProgress: UpdateProgress | null;
  setCloudUpdate: (v: CloudUpdateInfo | null) => void;
  setUpdateBusy: (v: boolean) => void;
  setUpdateProgress: (v: UpdateProgress | null) => void;
};

export const useUpdateStore = create<UpdateState>((set) => ({
  cloudUpdate: null,
  updateBusy: false,
  updateProgress: null,
  setCloudUpdate: (cloudUpdate) => set({ cloudUpdate }),
  setUpdateBusy: (updateBusy) => set({ updateBusy }),
  setUpdateProgress: (updateProgress) => set({ updateProgress }),
}));
