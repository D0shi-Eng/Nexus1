import { describe, expect, it } from 'vitest';
import { nexusModuleKeys } from '@nexus/contracts';
import { ar } from '../locales/ar';
import { en } from '../locales/en';
import { navigationItems, type NavigationKey } from './navigation';

describe('restored navigation contract', () => {
  it('publishes all 15 translated modules in both languages', () => {
    expect(nexusModuleKeys).toHaveLength(15);
    expect(Object.keys(en.navigation).sort()).toEqual([...nexusModuleKeys].sort());
    expect(Object.keys(ar.navigation).sort()).toEqual([...nexusModuleKeys].sort());
  });

  // انحدار مرئي حقيقي: أيقونتان متطابقتان لوحدتين مختلفتين تُلغيان التمييز البصري
  // في القائمة، وهو الغرض الوحيد للأيقونة — لذلك يُفرض التمايز لا يُوصى به.
  it('assigns one distinct semantic icon per module', () => {
    const icons = navigationItems.map(({ icon }) => icon);
    expect(new Set(icons).size).toBe(navigationItems.length);
    for (const { key, icon } of navigationItems) {
      expect(nexusModuleKeys).toContain(key);
      expect(icon).toBeTypeOf('object');
    }
  });

  it('covers every module key exactly once in the sidebar order', () => {
    const keys = navigationItems.map(({ key }) => key);
    expect(new Set(keys).size).toBe(nexusModuleKeys.length);
    const typed: readonly NavigationKey[] = keys;
    expect([...typed].sort()).toEqual([...nexusModuleKeys].sort());
  });
});
