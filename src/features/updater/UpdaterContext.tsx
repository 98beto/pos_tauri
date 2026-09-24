import { createContext, useContext, type ReactNode } from "react";

export type PosUpdateSafety = {
  cartNonEmpty: boolean;
  cartMutationInFlight: boolean;
  checkoutInFlight: boolean;
};

export type CartReportVersion = {
  generation: number;
  operation: number;
};

export type UpdaterControls = {
  manualCheck: () => void;
  checking: boolean;
  busy: boolean;
  beginCartReporting: (sessionId: number) => number;
  beginCartOperation: (sessionId: number, version: CartReportVersion) => boolean;
  reportCart: (sessionId: number, version: CartReportVersion, cartNonEmpty: boolean) => boolean;
  runCartMutation: <T>(sessionId: number, version: CartReportVersion, operation: () => Promise<T>) => Promise<T>;
  runCheckout: <T>(sessionId: number, operation: () => Promise<T>) => Promise<T>;
};

const defaultControls: UpdaterControls = {
  manualCheck: () => undefined,
  checking: false,
  busy: false,
  beginCartReporting: () => 0,
  beginCartOperation: () => true,
  reportCart: () => true,
  runCartMutation: (_sessionId, _version, operation) => operation(),
  runCheckout: (_sessionId, operation) => operation(),
};

export const UpdaterContext = createContext<UpdaterControls>(defaultControls);

export function useUpdater() {
  return useContext(UpdaterContext);
}

export function UpdaterProvider({ value, children }: { value: UpdaterControls; children: ReactNode }) {
  return <UpdaterContext.Provider value={value}>{children}</UpdaterContext.Provider>;
}
