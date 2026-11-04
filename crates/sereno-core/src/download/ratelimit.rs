//! 全局下载限速（令牌桶）。
//!
//! 限速必须是**全局**的：用户说的「限速 5MB/s」指整个启动器加起来，
//! 而不是每个文件 5MB/s——后者在 8 并发下会变成 40MB/s，形同虚设。
//! 因此用进程级静态桶，所有下载会话共享同一份令牌。

use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

/// 当前限速（KB/s）。0 表示不限速。
static LIMIT_KBPS: AtomicU64 = AtomicU64::new(0);

struct Bucket {
    tokens: f64,
    last: Instant,
}

static BUCKET: LazyLock<Mutex<Bucket>> = LazyLock::new(|| {
    Mutex::new(Bucket {
        tokens: 0.0,
        last: Instant::now(),
    })
});

/// 设置限速（KB/s，0 表示不限）。
pub fn set_limit_kbps(kbps: u64) {
    LIMIT_KBPS.store(kbps, Ordering::Relaxed);
}

/// 当前限速（KB/s）。
pub fn limit_kbps() -> u64 {
    LIMIT_KBPS.load(Ordering::Relaxed)
}

/// 在读取下一段数据前等待足够的令牌。
///
/// `bytes` 是即将写入的字节数。不限速时直接返回，没有任何额外开销。
pub async fn throttle(bytes: usize) {
    let kbps = limit_kbps();
    if kbps == 0 || bytes == 0 {
        return;
    }

    let rate = kbps as f64 * 1024.0; // 每秒字节数
    // 桶容量 = 1 秒的量，避免瞬时大块数据把节奏打乱
    let capacity = rate.max(4096.0);

    // **按容量分段计费**：一次读取往往是 64 KB，而桶最多只有 1 秒的量。
    // 早期写法用 `want = min(bytes, capacity)`，一块 64 KB 只扣 16 KB 的令牌
    // 就整块放行——限速 16 KB/s 时实际跑 64 KB/s（4 倍），设置项等于撒谎。
    let mut remaining = bytes as f64;
    while remaining > 0.0 {
        let want = remaining.min(capacity);
        remaining -= want;

        'wait: loop {
            let wait = {
                let mut bucket = BUCKET.lock().await;
                let now = Instant::now();
                let elapsed = now.duration_since(bucket.last).as_secs_f64();
                bucket.tokens = (bucket.tokens + elapsed * rate).min(capacity);
                bucket.last = now;

                if bucket.tokens >= want {
                    bucket.tokens -= want;
                    break 'wait;
                }

                let missing = want - bucket.tokens;
                // 上限 250ms：让取消信号与进度推送仍然及时
                Duration::from_secs_f64((missing / rate).min(0.25))
            };
            tokio::time::sleep(wait).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_means_unlimited() {
        set_limit_kbps(0);
        assert_eq!(limit_kbps(), 0);
    }

    #[tokio::test]
    async fn throttles_to_the_configured_rate() {
        // 64 KB/s，灌 32 KB 应该接近 0.5 秒
        set_limit_kbps(64);
        let start = Instant::now();
        throttle(32 * 1024).await;
        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(250),
            "32KB @64KB/s 至少要等 0.5s（含桶初始余量），实际 {elapsed:?}"
        );

        set_limit_kbps(0);
    }
}
