import type { TransRenderProps } from "@lingui/react";
import { I18nProvider as LinguiI18nProvider } from "@lingui/react";
import type * as React from "react";
import { Text } from "react-native";
import { i18n } from "./lingui";

const DefaultComponent = (props: TransRenderProps) => {
  return <Text>{props.children}</Text>;
};

export function LinguiProvider({ children }: { children: React.ReactNode }) {
  return (
    <LinguiI18nProvider i18n={i18n} defaultComponent={DefaultComponent}>
      {children}
    </LinguiI18nProvider>
  );
}
