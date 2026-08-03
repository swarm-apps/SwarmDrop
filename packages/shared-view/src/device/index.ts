export { transportLabel } from "./connection";
export { DEVICE_NAME_MAX_CHARS, deviceDisplayName, type DeviceNameSource } from "./name";
export {
  deviceGroupNames,
  deviceIdentityHint,
  emptyDeviceOrganization,
  hasDuplicateOrganizedName,
  normalizeDeviceOrganization,
  organizedDeviceName,
  shortPeerId,
  sortDeviceGroups,
  type DeviceGroup,
  type DeviceOrganization,
  type IdentifiedDevice,
} from "./organization";
export {
  canSendToDevice,
  normalizeTrustLevel,
  policyNoteFor,
  TRUST_LEVELS,
  type PolicyNote,
  type TrustLevel,
} from "./trust";
