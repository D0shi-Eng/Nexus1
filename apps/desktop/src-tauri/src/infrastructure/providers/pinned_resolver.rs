// =====================================================================
// مُحلِّل أسماء مُثبَّت (pinned resolver) — إغلاق ثغرة DNS rebinding
// ---------------------------------------------------------------------
// العيبة المُثبتة (كانت HIGH):
//   سياسة الوجهة (`NetworkPolicy::evaluate`) تحلّ المضيف وتتحقق من **كل** عنوان
//   ناتج، ثم كان الاتصال يُبنى من **الاسم** لا من العنوان المتحقق منه: `origin()`
//   يعيد `scheme://host` فقط، والمحرّك يعيد الحل بنفسه لحظة الاتصال. فينشأ سباق
//   (TOCTOU): اسم يعيد عنواناً حلقياً عند التحقق ثم عنوان المهاجم عند الاتصال
//   يمرّ من السياسة، وينتهي الطلب — ومعه ترويسة `Authorization` — إلى وجهة غير
//   مصرّح بها. وكان تعليق الوحدة يدّعي التثبيت دون أن ينفّذه أي سطر.
//
// الإصلاح:
//   `Resolver` مخصّص يعيد **العناوين المُتحقَّق منها المخزَّنة حصراً** ويعيد
//   المنفذ من الـURI فقط. لا يقع أي استعلام DNS ثانٍ، فتفشل إعادة التوجيه إلى
//   عنوان آخر فشلاً مغلقاً. وأي اختلاف بين المضيف الطلوب والمضيف المُتحقَّق منه
//   يُرفض أيضاً (فلا يُعاد استخدام نفس المُحلِّل لوجهة أخرى).
//
// ملاحظة حدود: التثبيت يحمي من تغيّر الحل بين التحقق والاتصال. لا يحمي من
// عنوان مُتحقَّق منه في الأصل كان خبيثاً — وهذا ما تمنعه قوائم الحظر في السياسة.
// =====================================================================

use std::net::SocketAddr;
use std::time::Duration;

use ureq::config::Config;
use ureq::http::Uri;
use ureq::unversioned::resolver::{ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{Connector, NextTimeout, RustlsConnector, TcpConnector};
use ureq::{Agent, Error};

use crate::domain::network_policy::AllowedDestination;

/// مُحلِّل يعيد العناوين المُتحقَّق منها وحدها.
#[derive(Debug)]
pub struct PinnedResolver {
    host: String,
    port: u16,
    addresses: Vec<SocketAddr>,
}

impl PinnedResolver {
    /// يُبنى من وجهة **سبق** أن مرّت سياسة الشبكة، فلا يُبنى من نص خام أبداً.
    pub fn new(destination: &AllowedDestination) -> Self {
        let port = destination.port();
        Self {
            host: destination.host().to_owned(),
            port,
            addresses: destination
                .resolved_addresses()
                .iter()
                .map(|ip| SocketAddr::new(*ip, port))
                .collect(),
        }
    }
}

/// المنفذ الافتراضي للمخطط — مكتوب صراحةً لتفادي الاعتماد على امتدادات
/// `http` غير المعاد تصديرها من هذه الحزمة. القيمتان ثابتتان في المعيار.
fn default_port_for_scheme(scheme: &str) -> Option<u16> {
    match scheme {
        "https" => Some(443),
        "http" => Some(80),
        _ => None,
    }
}

impl Resolver for PinnedResolver {
    fn resolve(
        &self,
        uri: &Uri,
        _config: &Config,
        _timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, Error> {
        // مطابقة المضيف حرفياً: لا نخدم اسم مضيف آخر بنفس العناوين المثبتة.
        match uri.host() {
            Some(host) if host == self.host => {}
            _ => return Err(Error::HostNotFound),
        }

        // المنفذ الطلوب يجب أن يطابق منفذ الوجهة المُتحقَّق منه.
        let requested_port = uri
            .port_u16()
            .or_else(|| uri.scheme_str().and_then(default_port_for_scheme));
        if requested_port != Some(self.port) {
            return Err(Error::HostNotFound);
        }

        let mut resolved = self.empty();
        for address in &self.addresses {
            resolved.push(*address);
        }

        if resolved.is_empty() {
            // لا عناوين مثبتة ⇒ لا اتصال. لا سقوط إلى الحل الافتراضي إطلاقاً.
            return Err(Error::HostNotFound);
        }

        Ok(resolved)
    }
}

/// وكيل HTTP مثبَّت الوجهة: القيود من سياسة النقل، والاتصال على العناوين
/// المُتحقَّق منها وحدها، وبلا وسيط وبلا تحويلات (سياسة النقل تضبطها).
pub fn create_pinned_agent(destination: &AllowedDestination, timeout: Duration) -> Agent {
    let limits = destination.limits();
    let config = Agent::config_builder()
        .timeout_global(Some(timeout))
        .timeout_connect(Some(limits.connect_timeout))
        .max_redirects(limits.max_redirects)
        .proxy(None)
        .build();
    // سلسلة الموصلات: TCP ثم TLS — نفس ترتيب المسار الافتراضي في ureq، بلا
    // وسيط وبلا موصل تجريبي. المُحلِّل مُثبَّت فلا يُعاد أي حل أسماء.
    let connector = ().chain(TcpConnector::default()).chain(RustlsConnector::default());
    Agent::with_parts(config, connector, PinnedResolver::new(destination))
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use ureq::config::Config;
    use ureq::http::Uri;
    use ureq::unversioned::resolver::Resolver;
    use ureq::unversioned::transport::time::Duration;
    use ureq::unversioned::transport::NextTimeout;
    use ureq::Timeout;

    use super::PinnedResolver;
    use crate::domain::network_policy::{AllowedDestination, TransportLimits};

    fn destination(host: &str, port: u16, addresses: Vec<IpAddr>) -> AllowedDestination {
        AllowedDestination::for_tests(
            "https",
            host,
            port,
            addresses,
            false,
            TransportLimits::default(),
        )
    }

    fn timeout() -> NextTimeout {
        NextTimeout {
            after: Duration::from_secs(5),
            reason: Timeout::Connect,
        }
    }

    fn resolve(
        host: &str,
        port: u16,
        addresses: Vec<IpAddr>,
        url: &str,
    ) -> Result<Vec<String>, ()> {
        let resolver = PinnedResolver::new(&destination(host, port, addresses));
        let uri = Uri::try_from(url).expect("عنوان الاختبار يجب أن يُحلَّل");
        resolver
            .resolve(&uri, &Config::default(), timeout())
            .map(|resolved| resolved.iter().map(|addr| addr.to_string()).collect())
            .map_err(|_| ())
    }

    // الدليل الجوهري على التثبيت: المضيف داخل نطاق `.invalid` المحجوز **لا يُحلّ
    // في DNS إطلاقاً**، ومع ذلك يعيد المُحلِّل عنواناً — أي أنه لم يستشر DNS بل
    // استعمل العناوين المُتحقَّق منها المخزَّنة. لو بقي المسار القديم (إعادة الحل
    // عند الاتصال) لفشل هذا الطلب بـ HostNotFound.
    #[test]
    fn resolves_from_the_pinned_addresses_without_any_dns_lookup() {
        let resolved = resolve(
            "unresolvable.invalid",
            443,
            vec![IpAddr::V4(Ipv4Addr::new(203, 0, 113, 7))],
            "https://unresolvable.invalid/v1/chat/completions",
        )
        .expect("التثبيت يجب أن يعيد العنوان بلا DNS");
        assert_eq!(resolved, vec!["203.0.113.7:443".to_owned()]);
    }

    // عناوين مثبتة لوجهة ما لا تُخدَم لوجهة أخرى — لو أُعيد استخدام المُحلِّل
    // لاسم آخر لصار التثبيت باباً للاتصال بعنوان غير مُتحقَّق منه لهذا الاسم.
    #[test]
    fn refuses_a_different_host_even_though_addresses_are_pinned() {
        assert!(
            resolve(
                "valid.example",
                443,
                vec![IpAddr::V4(Ipv4Addr::new(203, 0, 113, 7))],
                "https://attacker.example/v1/models",
            )
            .is_err(),
            "مضيف مختلف يجب أن يُرفض"
        );
    }

    // المنفذ جزء من الوجهة: لا يُعاد توجيه نفس المضيف إلى منفذ آخر.
    #[test]
    fn refuses_a_port_other_than_the_validated_one() {
        assert!(
            resolve(
                "valid.example",
                443,
                vec![IpAddr::V4(Ipv4Addr::new(203, 0, 113, 7))],
                "https://valid.example:8443/v1/models",
            )
            .is_err(),
            "منفذ مختلف يجب أن يُرفض"
        );
    }

    // لا عناوين مثبتة ⇒ لا اتصال. لا سقوط إلى الحل الافتراضي في أي حال.
    #[test]
    fn refuses_when_the_validated_address_set_is_empty() {
        assert!(
            resolve(
                "valid.example",
                443,
                vec![],
                "https://valid.example/v1/models"
            )
            .is_err(),
            "غياب عناوين مثبتة يجب أن يفشل مغلقاً"
        );
    }
}
