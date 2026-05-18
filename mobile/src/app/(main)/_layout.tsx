import { Drawer } from "expo-router/drawer";
import { DrawerContent } from "@/components/drawer-content";

/**
 * (main) 层只承载主屏一个 Screen。Drawer 用作主屏左侧的全局入口面板。
 * 传输历史 / 设置 / 关于 等都是 root Stack 的同级路由,由 Drawer 项 router.push 跳转。
 */
export default function MainLayout() {
  return (
    <Drawer
      drawerContent={(props) => <DrawerContent {...props} />}
      screenOptions={{ headerShown: false, drawerType: "front" }}
    >
      <Drawer.Screen name="index" />
    </Drawer>
  );
}
