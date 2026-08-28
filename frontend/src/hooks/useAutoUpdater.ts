import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export function useAutoUpdater(componentName: string = 'App') {
  const [updateAvailable, setUpdateAvailable] = useState<string | null>(null);

  useEffect(() => {
    console.log(`[useAutoUpdater:${componentName}] Hook mounted. Querying Rust get_available_update...`);

    invoke<string | null>('get_available_update')
      .then((version) => {
        console.log(`[useAutoUpdater:${componentName}] get_available_update resolved with:`, version);
        if (version) {
          setUpdateAvailable(version);
        }
      })
      .catch((err) => {
        console.error(`[useAutoUpdater:${componentName}] get_available_update error:`, err);
      });

    let unlisten: (() => void) | undefined;
    listen<string | null>('update-available', (event) => {
      console.log(`[useAutoUpdater:${componentName}] Received update-available event:`, event.payload);
      setUpdateAvailable(event.payload ?? null);
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch((err) => {
        console.error(`[useAutoUpdater:${componentName}] Failed to listen to event:`, err);
      });

    return () => {
      console.log(`[useAutoUpdater:${componentName}] Hook unmounting.`);
      if (unlisten) {
        unlisten();
      }
    };
  }, [componentName]);

  return { updateAvailable, setUpdateAvailable };
}
