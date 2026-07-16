import { Redirect } from "expo-router";

export default function TransferIndexRedirect() {
  return <Redirect href={"/activity" as never} />;
}
