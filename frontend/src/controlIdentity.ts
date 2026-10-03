import { watch, type WatchSource } from 'vue';

// Compare identity values, not the newly allocated array or calibration object.
export function watchControlIdentity(sources: WatchSource[], reset: () => void) {
  return watch(sources, reset);
}
