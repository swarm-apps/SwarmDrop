## 1. TransferOfferDialog Component

- [x] 1.1 Create `src/components/transfer/transfer-offer-dialog.tsx` with ResponsiveDialog
- [x] 1.2 Add dialog header with icon, title "收到文件", and sender device name
- [x] 1.3 Integrate FileTree component to show file preview (mode="select")
- [x] 1.4 Add save path display with "更改" button
- [x] 1.5 Add "拒绝" and "接收" action buttons
- [x] 1.6 Handle accept action: call acceptReceive, add session, navigate to detail page
- [x] 1.7 Handle reject action: call rejectReceive, close dialog, show next offer

## 2. Remove Auto-Navigation

- [x] 2.1 Modify `src/routes/_app.tsx` to remove auto-navigation to `/receive`
- [x] 2.2 Add TransferOfferDialog component to AppLayout
- [x] 2.3 Ensure dialog shows when pendingOffers.length > 0

## 3. Transfer Detail Page

- [x] 3.1 Create route file `src/routes/_app/transfer/$sessionId.tsx`
- [x] 3.2 Create lazy component `src/routes/_app/transfer/$sessionId.lazy.tsx`
- [x] 3.3 Implement session lookup from transfer-store by sessionId param
- [x] 3.4 Add "Transferring" state UI: progress bar, speed, ETA, file tree
- [x] 3.5 Add "Completed" state UI: success icon, stats, "打开文件夹" button
- [x] 3.6 Add "Failed" state UI: error icon, error message, retry button
- [x] 3.7 Add "Cancelled" state UI: cancelled message
- [x] 3.8 Add cancel button for active transfers

## 4. TransferItem Navigation

- [x] 4.1 Modify `src/routes/_app/transfer/-transfer-item.tsx`
- [x] 4.2 Make the card clickable to navigate to `/transfer/:sessionId`
- [x] 4.3 Ensure click doesn't conflict with action buttons (cancel, open folder)

## 5. Responsive Design

- [x] 5.1 Test TransferOfferDialog on mobile (should use bottom sheet)
- [x] 5.2 Test TransferOfferDialog on desktop (should use centered dialog)
- [x] 5.3 Test TransferDetailPage on mobile layout
- [x] 5.4 Test TransferDetailPage on desktop layout

## 6. Optional: Deprecate /receive Page

- [x] 6.1 Decide whether to keep or remove `/receive` route - 保留作为备用入口
- [x] 6.2 If keeping, update to show queue of pending offers - 保持现状，用户可手动访问
- [x] 6.3 If removing, delete or redirect the route - 暂不删除
