import { ref } from "vue";

/**
 * 页面切换方向，用于让路由过渡「有来源」。
 *
 * 主导航是横向排列的，所以「从主页切到实例」应该是新页面**从右侧推入**；
 * 反过来则从左侧推入。之前所有切换都只有一个淡入淡出，
 * 用户感受不到「往前走」还是「往回退」。
 *
 * 位移只有 8px——整屏平移会让人眼花，8px 足够表达方向。
 */

/** 主导航顺序，必须与 `components/layout/nav.ts` 的 primaryItems 一致。 */
const ORDER = ["/", "/instances", "/downloads", "/tasks", "/settings", "/more"];

/** 1 = 向右（新页从右进），-1 = 向左，0 = 纯淡入。 */
const direction = ref(0);

/** 当前应使用的过渡名。 */
export function transitionName(): string {
  if (direction.value > 0) return "page-right";
  if (direction.value < 0) return "page-left";
  return "page-fade";
}

/** 在 `router.afterEach` 里调用。 */
export function updateNavDirection(from: string, to: string): void {
  const fromIndex = ORDER.indexOf(from);
  const toIndex = ORDER.indexOf(to);

  // 任一端不是主导航页面（例如未来的实例详情页）→ 退化为淡入
  direction.value =
    fromIndex === -1 || toIndex === -1 || fromIndex === toIndex
      ? 0
      : (Math.sign(toIndex - fromIndex) as -1 | 0 | 1);
}

export function useNavDirection() {
  return { direction, transitionName };
}
