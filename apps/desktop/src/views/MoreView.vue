<script setup lang="ts">
import { ref } from "vue";
import { useRouter } from "vue-router";
import {
  CircleHelp,
  Heart,
  Info,
  HeartHandshake,
  MessageSquare,
  RefreshCw,
} from "@lucide/vue";

import AppButton from "@/components/common/AppButton.vue";
import AppCard from "@/components/common/AppCard.vue";
import AppEmptyState from "@/components/common/AppEmptyState.vue";
import PageShell from "@/components/layout/PageShell.vue";
import SideNav from "@/components/layout/SideNav.vue";
import type { SideNavItem } from "@/components/layout/SideNav.vue";
import { APP_NAME, APP_VERSION } from "@/constants/app";
import { log } from "@/api/settings";
import { openProjectIssues, openProjectRepo, openSupporterPage, type SupporterTarget } from "@/api/supporters";

/** 「更多」页：关于 / 鸣谢 / 感谢名单 / 帮助 / 反馈。 */
type SectionId = "about" | "credits" | "thanks" | "help" | "feedback";

const sections: SideNavItem[] = [
  { id: "about", label: "关于", icon: Info },
  { id: "credits", label: "鸣谢", icon: HeartHandshake },
  { id: "thanks", label: "感谢", icon: Heart },
  { id: "help", label: "帮助", icon: CircleHelp },
  { id: "feedback", label: "反馈", icon: MessageSquare },
];

const activeSection = ref<SectionId>("about");
const router = useRouter();

/** 跳到更新页。 */
function goUpdate(): void {
  void router.push("/update");
}

/** 赞助名单：后续有赞助者时把名字填进来即可。 */
interface Sponsor {
  name: string;
  note: string;
}

const sponsors: Sponsor[] = [];

interface Acknowledgement {
  name: string;
  desc: string;
  supporter: SupporterTarget | null;
}

const acknowledgements: Acknowledgement[] = [
  {
    name: "BMCLAPI",
    desc: "bangbang93 维护的 Minecraft 资源镜像，为游戏本体、加载器与 Maven 依赖提供国内加速。",
    supporter: "bmclapi",
  },
  {
    name: "MCIM",
    desc: "面向国内用户的 Modrinth 与 CurseForge 元数据及文件镜像。",
    supporter: "mcim",
  },
  {
    name: "PCL2",
    desc: "加载器离线安装、镜像优先与实例资源管理的思路参考了其设计。",
    supporter: "pcl",
  },
];

function sponsorFor(target: SupporterTarget | null): void {
  if (!target) return;
  void openSupporterPage(target).catch((error) => {
    void log.error(`打开赞助页失败: ${String(error)}`, { component: "MoreView" });
  });
}

function openRepo(): void {
  void openProjectRepo().catch((error) => {
    void log.error(`打开仓库失败: ${String(error)}`, { component: "MoreView" });
  });
}

function openIssues(): void {
  void openProjectIssues().catch((error) => {
    void log.error(`打开反馈页失败: ${String(error)}`, { component: "MoreView" });
  });
}
</script>

<template>
  <PageShell :chrome="false" width="xl">
    <div class="flex min-h-0 flex-1 gap-6">
      <SideNav v-model="activeSection" :items="sections" title="更多" />

      <div class="min-h-0 flex-1 overflow-y-auto pb-2">
        <Transition name="tab" mode="out-in">
          <!-- 关于 -->
          <AppCard v-if="activeSection === 'about'" key="about" padding="lg">
            <div class="flex flex-col items-center py-2 text-center">
              <!-- Logo：不再有承载块、光晕与描边，只留标志本身 -->
              <div class="about-logo">
                <svg
                  viewBox="0 0 100 100"
                  class="relative h-16 w-16 shrink-0 text-brand-500"
                  role="img"
                  aria-label="Sereno"
                >
                  <defs>
                    <mask
                      id="sereno-about-cut"
                      maskUnits="userSpaceOnUse"
                      x="0"
                      y="0"
                      width="100"
                      height="100"
                    >
                      <rect width="100" height="100" fill="#fff" />
                      <rect
                        x="36"
                        y="45.5"
                        width="28"
                        height="5"
                        fill="#000"
                        transform="rotate(65 50 48)"
                      />
                    </mask>
                  </defs>
                  <path
                    d="M 72 24 L 38 24 A 12 12 0 0 0 38 48 L 62 48 A 12 12 0 0 1 62 72 L 28 72"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="14"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    mask="url(#sereno-about-cut)"
                  />
                </svg>
              </div>

              <h2 class="mt-5 font-display text-display font-semibold text-ink">{{ APP_NAME }}</h2>
              <p class="mt-1.5 max-w-sm text-caption leading-6 text-ink-muted">
                一个专注「装得对、跑得稳、坏得少」的 Minecraft 启动器。
              </p>

              <div class="mt-5 flex items-center gap-2">
                <AppButton variant="secondary" size="sm" @click="openRepo">
                  <svg viewBox="0 0 16 16" class="h-3.5 w-3.5" fill="currentColor" aria-hidden="true">
                    <path
                      d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0 0 16 8c0-4.42-3.58-8-8-8Z"
                    />
                  </svg>
                  GitHub 仓库
                </AppButton>
                <AppButton variant="soft" size="sm" @click="sponsorFor('sereno')">
                  <HeartHandshake :size="14" />
                  赞助
                </AppButton>
              </div>

              <!--
                版本 / 许可证 / 技术栈收成底部一行等宽小字。
                三列并排的「信息卡」看着像内部测试版的草稿，
                一行带分隔线的等宽字才是正式开源项目的样子。
              -->
              <!--
                授权声明：用大白话把「红线」画出来。
                协议本身在 LICENSE / README，这里负责让每个用户都知道
                「免费、无广告、不联机」是承诺，也是边界。
              -->
              <div class="mt-8 flex flex-col items-center gap-3 border-t border-line/60 pt-5">
                <div class="flex flex-wrap items-center justify-center gap-2 font-mono text-micro text-ink-muted">
                  <span>v{{ APP_VERSION }}</span>
                  <span class="h-3 w-px bg-line" />
                  <span>PolyForm Noncommercial 1.0.0</span>
                  <span class="h-3 w-px bg-line" />
                  <span>Built with Tauri 2 &amp; Vue 3</span>
                </div>

                <div
                  class="flex max-w-xl flex-col gap-1.5 text-center text-micro leading-5 text-ink-muted"
                >
                  <p>
                    本项目基于
                    <strong class="font-semibold text-ink-soft">PolyForm Noncommercial License</strong>
                    开源，
                    <strong class="font-semibold text-ink-soft">严禁任何形式的商业倒卖与盈利行为</strong>
                    （包括但不限于套壳售卖、绑定私服充值、投放商业广告、捆绑付费插件）。
                  </p>
                  <p>
                    欢迎所有热爱 Minecraft 的开发者参考、借鉴本项目的代码机制与架构设计，
                    一切以学习、研究与非商业交流为目的的二次开发都被鼓励。
                  </p>
                  <p class="opacity-80">
                    Sereno 是一款完全免费、无广告、无遥测的第三方启动器。
                    本软件与 Mojang Studios、Microsoft 或网易公司无关。
                  </p>
                </div>
              </div>

              <!-- 更新入口：更新功能有自己的页面，这里只给一个跳转 -->
              <div class="mt-5 w-full max-w-xl">
                <AppButton variant="soft" size="sm" @click="goUpdate">
                  <RefreshCw :size="13" />
                  检查更新
                </AppButton>
              </div>
            </div>
          </AppCard>

          <!-- 鸣谢 -->
          <AppCard v-else-if="activeSection === 'credits'" key="credits" title="特别鸣谢" padding="lg">
            <template #icon>
              <HeartHandshake :size="16" class="text-brand-500" />
            </template>

            <div class="flex flex-col divide-y divide-line/50">
              <div
                v-for="item in acknowledgements"
                :key="item.name"
                class="flex items-center justify-between gap-6 py-4"
              >
                <div class="min-w-0">
                  <h3 class="text-h2 font-semibold text-ink">{{ item.name }}</h3>
                  <p class="mt-1 max-w-xl text-caption leading-6 text-ink-muted">{{ item.desc }}</p>
                </div>
                <AppButton
                  v-if="item.supporter"
                  variant="ghost"
                  size="sm"
                  @click="sponsorFor(item.supporter)"
                >
                  <HeartHandshake :size="13" />
                  赞助
                </AppButton>
              </div>
            </div>
          </AppCard>

          <!-- 感谢名单 -->
          <AppCard v-else-if="activeSection === 'thanks'" key="thanks" title="感谢" padding="lg">
            <template #icon>
              <Heart :size="16" class="text-brand-500" />
            </template>

            <p class="mb-2 text-caption text-ink-muted">每一份支持都会被记在这里。</p>

            <AppEmptyState
              v-if="sponsors.length === 0"
              :icon="Heart"
              title="还没有赞助名单"
              description="如果你愿意请我喝杯奶茶，名字会出现在这里。不赞助也完全没关系，用得开心就好。"
              action-label="赞助 Sereno"
              @action="sponsorFor('sereno')"
            />

            <div v-else class="flex flex-col divide-y divide-line/50">
              <div
                v-for="sponsor in sponsors"
                :key="sponsor.name"
                class="flex items-center justify-between gap-6 py-3.5"
              >
                <div class="min-w-0">
                  <p class="truncate text-body font-semibold text-ink">{{ sponsor.name }}</p>
                  <p v-if="sponsor.note" class="mt-0.5 text-caption text-ink-muted">
                    {{ sponsor.note }}
                  </p>
                </div>
                <Heart :size="14" class="shrink-0 text-brand-500/60" />
              </div>
            </div>
          </AppCard>

          <!-- 帮助 -->
          <AppCard v-else-if="activeSection === 'help'" key="help" title="帮助中心" padding="lg">
            <template #icon>
              <CircleHelp :size="16" class="text-brand-500" />
            </template>

            <AppEmptyState
              :icon="CircleHelp"
              title="常见问题正在整理"
              description="Java 选择、Mod 冲突、崩溃排查的说明会在后续版本里开放。"
            />
          </AppCard>

          <!-- 反馈 -->
          <AppCard v-else key="feedback" title="反馈" padding="lg">
            <template #icon>
              <MessageSquare :size="16" class="text-brand-500" />
            </template>

            <div class="flex flex-col gap-4">
              <div class="flex items-center gap-3 rounded-md bg-surface-muted px-4 py-3.5">
                <div
                  class="flex h-10 w-10 shrink-0 items-center justify-center rounded-md bg-brand-500/10 text-brand-500"
                >
                  <MessageSquare :size="18" />
                </div>
                <div class="min-w-0 flex-1">
                  <p class="text-body font-semibold text-ink">GitHub Issues</p>
                  <p class="text-micro text-ink-muted">问题反馈、功能建议、使用疑问</p>
                </div>
                <AppButton variant="primary" size="sm" @click="openIssues">前往反馈</AppButton>
              </div>

              <p class="text-micro leading-5 text-ink-muted">
                反馈前建议先在「实例 → 备份与日志」导出诊断包，并附上版本号 v{{ APP_VERSION }}、复现步骤与截图。
              </p>
            </div>
          </AppCard>
        </Transition>
      </div>
    </div>
  </PageShell>
</template>

<style scoped>
/* 只有标志本身：不要底色块、不要描边、不要光晕 */
.about-logo {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
}

</style>
