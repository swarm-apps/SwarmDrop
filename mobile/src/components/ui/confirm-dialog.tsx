/**
 * ConfirmDialog —— 受控的「取消 / 确认」二选一对话框。
 *
 * 替代分散在各页的 `Alert.alert(title, msg, [{...cancel}, {...action}])`，
 * 统一观感（dark theme、blur overlay、destructive 红色按钮）。
 */

import { Trans } from "@lingui/react/macro";
import type { ReactNode } from "react";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Text } from "@/components/ui/text";

interface ConfirmDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: ReactNode;
  description?: ReactNode;
  cancelLabel?: ReactNode;
  actionLabel: ReactNode;
  /** true 时确认按钮渲染为红色 destructive variant */
  destructive?: boolean;
  onAction: () => void;
  contentTestID?: string;
  cancelTestID?: string;
  actionTestID?: string;
}

export function ConfirmDialog({
  open,
  onOpenChange,
  title,
  description,
  cancelLabel,
  actionLabel,
  destructive,
  onAction,
  contentTestID,
  cancelTestID,
  actionTestID,
}: ConfirmDialogProps) {
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent testID={contentTestID}>
        <AlertDialogHeader>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          {description ? (
            <AlertDialogDescription>{description}</AlertDialogDescription>
          ) : null}
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel testID={cancelTestID}>
            <Text>{cancelLabel ?? <Trans>取消</Trans>}</Text>
          </AlertDialogCancel>
          <AlertDialogAction
            variant={destructive ? "destructive" : undefined}
            onPress={onAction}
            testID={actionTestID}
          >
            <Text>{actionLabel}</Text>
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
