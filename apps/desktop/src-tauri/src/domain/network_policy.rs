// =====================================================================
// سياسة الوجهة الشبكية (Network / destination policy)
// ---------------------------------------------------------------------
// سياسة نقية قابلة للاختبار وحيدة المسؤولية: تأخذ عنواناً أساسياً مقدَّماً من
// المستخدم وتُصدر إمّا «وجهة مسموحة» ببيانات مُثبَّتة، وإمّا رمز رفض آمن ثابت.
//
// ما تحميه هذه السياسة فعلاً:
//   1) منع المخططات غير http/https (file:, ftp:, data:, ws: …).
//   2) منع أي بيانات اعتماد داخل العنوان (userinfo) لأنها تظهر في السجلات والأخطاء.
//   3) منع النقل غير المشفّر (http) إلى أي مضيف غير حلقي (loopback).
//   4) منع نطاقات IPv4/IPv6 الخطرة: link-local، multicast، unspecified،
//      والخاصة، وخدمات بيانات السحابة الوصفية (169.254.169.254 و fe80::/10 و 0.0.0.0).
//   5) حلّ المضيف والتحقق من كل عنوان ناتج — دفاع أساسي ضد DNS rebinding
//      (يُثبَّت العنوان المتحقق منه ويُبنى الاتصال عليه، فلا يُعاد الحل).
//   6) فرض قيود النقل: صفر تحويلات، مهلة اتصال ومهلة كلية، حدّ جسم، بلا وسيط.
//
// ما لا تحميه (تصريح صريح حتى لا يُدَّعى زوراً):
//   - لا تمنع مضيفاً عاماً وسيطاً (proxy) موجهاً عمداً من المستخدم إلى خدمة
//     سحابية مشروعة؛ المنع هنا للنطاقات الداخلية/الوصفية فقط.
//   - لا تحمي من سيطرة كاملة على DNS حيث يعيد الخادم عنواناً مسموحاً ثم
//     يغيّره لاحقاً: التثبيت يمنع «إعادة الحل بعد التحقق» في هذا الطلب فقط،
//     ولا يمنع هجوماً على مستوى الموجّه أو خادم DNS مخترق يعيد عنواناً خاصاً
//     يُقبل لأنه غير مدرج في قوائم المنع (المنع قائم على القوائم لا على الثقة).
//   - لا تفحص الشهادات الرقمية ولا تحلّ محل مدقّق TLS (التدقيق هنا لـ rustls).
//   - لا تحدّ من منافذ المضيف الحلقي: منفذ محلي مفتوح يبقى قابلاً للوصول،
//     وهو سلوك مقصود لمزوّدي النماذج المحليين (Ollama/LM Studio).
// =====================================================================

use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};
use std::time::Duration;

/// اسم المضيف الحلقي المعياري الوحيد المسموح نصّاً (بقية الحلقيات تُقبل كعناوين IP).
pub const LOCALHOST_NAME: &str = "localhost";

/// حدّ جسم الاستجابة الافتراضي لمسارات المزودين — يُطبَّق دائماً بلا استثناء.
pub const DEFAULT_MAX_RESPONSE_BYTES: u64 = 1024 * 1024;

/// التحويلات ممنوعة منعاً مطلقاً: تحويلة واحدة تكفي لتجاوز فحص الوجهة.
pub const MAX_REDIRECTS: u32 = 0;

/// مهلة الاتصال الافتراضية — قصيرة عمداً (فشل سريع بدل تعليق الواجهة).
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// المهلة الكلية الافتراضية للطلب الكامل.
pub const DEFAULT_GLOBAL_TIMEOUT: Duration = Duration::from_secs(30);

/// رموز الرفض الآمنة — ثابتة، إنجليزية، وخالية من أي عنوان أو منفذ أو مضيف.
/// لا تُدرج أي منها أي جزء من العنوان المرفوض ولا أي عنوان IP محلول.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkPolicyRejection {
    /// العنوان لم يُحلَّل أصلاً أو بُني بصيغة غير صالحة.
    BaseUrlInvalid,
    /// المخطط ليس http ولا https.
    SchemeNotAllowed,
    /// العنوان يحمل userinfo (اسم مستخدم/كلمة مرور) — مرفوض دائماً.
    CredentialsInUrl,
    /// المضيف ليس حلقياً لكن العنوان http — النقل غير المشفّر مرفوض.
    NonLoopbackHttpForbidden,
    /// الوجهة داخل نطاق محظور (link-local/متعدد البث/غير محدد/خاص/وصفي).
    DestinationDenied,
    /// تعذّر حلّ المضيف.
    HostUnresolved,
    /// المُثبِّت لم يُرجع أي عنوان.
    NoResolvedAddress,
}

impl NetworkPolicyRejection {
    /// الرمز الآمن الوحيد الذي يجوز أن يصل إلى الواجهة أو السجل.
    pub fn code(self) -> &'static str {
        match self {
            Self::BaseUrlInvalid => "NETWORK_BASE_URL_INVALID",
            Self::SchemeNotAllowed => "NETWORK_SCHEME_NOT_ALLOWED",
            Self::CredentialsInUrl => "NETWORK_CREDENTIALS_IN_URL",
            Self::NonLoopbackHttpForbidden => "NETWORK_NON_LOOPBACK_HTTP_FORBIDDEN",
            Self::DestinationDenied => "NETWORK_DESTINATION_DENIED",
            Self::HostUnresolved => "NETWORK_HOST_UNRESOLVED",
            Self::NoResolvedAddress => "NETWORK_NO_RESOLVED_ADDRESS",
        }
    }
}

/// قيود النقل المفروضة — تُبنى منها وكلاء HTTP حصراً، فلا يُنشئ أي مسار
/// اتصالاً خارج هذه الحدود.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportLimits {
    pub max_redirects: u32,
    pub connect_timeout: Duration,
    pub global_timeout: Duration,
    pub max_response_bytes: u64,
    /// بلا وسيط دائماً — الوسيط يفتح مساراً خارج فحص الوجهة.
    pub allow_proxy: bool,
}

impl Default for TransportLimits {
    fn default() -> Self {
        Self {
            max_redirects: MAX_REDIRECTS,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            global_timeout: DEFAULT_GLOBAL_TIMEOUT,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            allow_proxy: false,
        }
    }
}

/// وجهة تم التحقق منها بالكامل: تُحمل العناوين المثبَّتة والمضيف المطبَّع
/// والمخطط، ولا تُحمل أي مادة سرية.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowedDestination {
    /// المخطط المطبَّع: "http" أو "https".
    scheme: String,
    /// المضيف المطبَّع بلا أقواس IPv6 (كما يعرضه url::Host).
    host: String,
    /// المنفذ الفعلي بعد تطبيق منفذ المخطط الافتراضي.
    port: u16,
    /// العناوين التي وُثِّق أنها كلها ضمن النطاقات المسموحة.
    resolved_addresses: Vec<IpAddr>,
    /// هل المضيف حلقي صراحةً (localhost أو عنوان حلقي)؟
    loopback: bool,
    limits: TransportLimits,
}

impl AllowedDestination {
    pub fn limits(&self) -> &TransportLimits {
        &self.limits
    }

    /// الأصل المعياري (scheme://host[:port]) — يُستعمل لبناء مسارات المزود.
    /// المنفذ الافتراضي للمخطط (80 لـ http و443 لـ https) يُحذف لأن ذكره صراحةً
    /// يخالف ما يتوقعه المستخدم في العنوان المعروض ويكرر معلومة ضمنية.
    /// لا يحمل أي بيانات اعتماد لأن السياسة رفضت كل عنوان يحملها.
    pub fn origin(&self) -> String {
        let default_port = match self.scheme.as_str() {
            "https" => 443,
            _ => 80,
        };
        if self.port == default_port {
            format!("{}://{}", self.scheme, self.host)
        } else {
            format!("{}://{}:{}", self.scheme, self.host, self.port)
        }
    }

    /// هل العنوان https؟ القرار مبني على المخطط لا على المضيف.
    pub fn is_secure(&self) -> bool {
        self.scheme == "https"
    }

    /// المضيف المطبَّع (بلا أقواس IPv6) — يحتاجه المُحلِّل المُثبَّت ليطابق
    /// المضيف الطلوب بالمضيف المُتحقَّق منه، فلا يقع أي حل DNS ثانٍ.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// المنفذ الفعلي بعد تطبيق منفذ المخطط الافتراضي.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// العناوين التي وُثِّق أنها كلها ضمن النطاقات المسموحة. يُبنى الاتصال عليها
    /// **حصراً** عبر `pinned_resolver`، وهذا هو ما يجعل التحقق فعّالاً لا شكلياً:
    /// لولا ذلك لأعاد محرّك HTTP حلّ الاسم لحظة الاتصال وضاعت نتيجة التحقق
    /// (نافذة سباق DNS rebinding).
    pub fn resolved_addresses(&self) -> &[IpAddr] {
        &self.resolved_addresses
    }

    /// هل المضيف حلقي صراحةً؟ وصولة اختبار: سياسة النقل تقرأ الحلقية داخلياً
    /// ولا يحتاجها أي مسار إنتاج خارج هذه الوحدة.
    #[cfg(test)]
    pub fn is_loopback(&self) -> bool {
        self.loopback
    }

    /// حامل اختبارات فقط: يبني وجهة مباشرة بلا حلّ أسماء. مخصص لاختبارات
    /// المحوّلات التي تستهدف خادماً وهمياً على منفذ عشوائي — سياسة الوجهة نفسها
    /// لها اختباراتها المستقلة عبر `NetworkPolicy::evaluate`.
    #[cfg(test)]
    pub fn for_tests(
        scheme: &str,
        host: &str,
        port: u16,
        resolved_addresses: Vec<IpAddr>,
        loopback: bool,
        limits: TransportLimits,
    ) -> Self {
        Self {
            scheme: scheme.to_owned(),
            host: host.to_owned(),
            port,
            resolved_addresses,
            loopback,
            limits,
        }
    }
}

/// واجهة حلّ الأسماء — تُحقن لتُختبر السياسة بلا شبكة.
pub trait HostResolver {
    fn resolve(&self, host: &str, port: u16) -> Result<Vec<IpAddr>, NetworkPolicyRejection>;
}

/// حلّال الإنتاج: نظام الأسماء عبر ToSocketAddrs، مع إزالة التكرار وترتيب حتمي.
pub struct SystemResolver;

impl HostResolver for SystemResolver {
    fn resolve(&self, host: &str, port: u16) -> Result<Vec<IpAddr>, NetworkPolicyRejection> {
        let iterator = (host, port)
            .to_socket_addrs()
            .map_err(|_| NetworkPolicyRejection::HostUnresolved)?;
        let addresses = iterator.map(|socket| socket.ip()).collect::<BTreeSet<_>>();
        if addresses.is_empty() {
            return Err(NetworkPolicyRejection::NoResolvedAddress);
        }

        Ok(addresses.into_iter().collect())
    }
}

/// حلّال اختبارات حتمي: يربط مضيفاً بعناوين معطاة صراحةً.
#[cfg(test)]
pub struct StaticResolver {
    entries: Vec<(String, Vec<IpAddr>)>,
}

#[cfg(test)]
impl StaticResolver {
    pub fn new(entries: Vec<(&str, Vec<IpAddr>)>) -> Self {
        Self {
            entries: entries
                .into_iter()
                .map(|(host, addresses)| (host.to_owned(), addresses))
                .collect(),
        }
    }
}

#[cfg(test)]
impl HostResolver for StaticResolver {
    fn resolve(&self, host: &str, _port: u16) -> Result<Vec<IpAddr>, NetworkPolicyRejection> {
        self.entries
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(host))
            .map(|(_, addresses)| addresses.clone())
            .filter(|addresses| !addresses.is_empty())
            .ok_or(NetworkPolicyRejection::HostUnresolved)
    }
}

/// مدخل السياسة الوحيد: عنوان أساسي من المستخدم + قيود نقل مضبوطة.
pub struct NetworkPolicy<'a, R: HostResolver> {
    resolver: &'a R,
    limits: TransportLimits,
}

impl<'a, R: HostResolver> NetworkPolicy<'a, R> {
    pub fn new(resolver: &'a R) -> Self {
        Self {
            resolver,
            limits: TransportLimits::default(),
        }
    }

    #[cfg(test)]
    pub fn with_limits(resolver: &'a R, limits: TransportLimits) -> Self {
        Self { resolver, limits }
    }

    /// الفحص الكامل. ترتيب الفحوص ثابت ومقصود: بنية ← اعتماد ← مخطط ← تشفير ← حظر ← حلّ.
    pub fn evaluate(&self, base_url: &str) -> Result<AllowedDestination, NetworkPolicyRejection> {
        let trimmed = base_url.trim();
        if trimmed.is_empty() || trimmed.len() > 512 {
            return Err(NetworkPolicyRejection::BaseUrlInvalid);
        }
        // رفض صريح للأسطر الجديدة والمسافات الداخلية: url يُسقط بعض المحارف
        // الصامتة (tab/newline) فيصبح `http://exa\nmple.com` مقبولاً — نرفض قبل التحليل.
        if trimmed
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
        {
            return Err(NetworkPolicyRejection::BaseUrlInvalid);
        }

        let parsed =
            url::Url::parse(trimmed).map_err(|_| NetworkPolicyRejection::BaseUrlInvalid)?;
        // فحص بنيوي لبيانات الاعتماد على النص الخام: مُحلِّل WHATWG يُسقط
        // الاعتماد الفارغ (`https://:@host`) تماماً فلا يظهر في username().
        if authority_carries_credentials(trimmed) {
            return Err(NetworkPolicyRejection::CredentialsInUrl);
        }

        let scheme = parsed.scheme();
        if scheme != "http" && scheme != "https" {
            return Err(NetworkPolicyRejection::SchemeNotAllowed);
        }

        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err(NetworkPolicyRejection::CredentialsInUrl);
        }

        // المسار/الاستعلام/الجزء: الأساس يجب أن يكون أصلاً بلا مسار. جذر "/"
        // مقبول لأنه تطبيعي في صيغة WHATWG. ما عداه يرفض (يمنع مسارات تُخفي هدفاً).
        let path = parsed.path();
        if !path.is_empty() && path != "/" {
            return Err(NetworkPolicyRejection::BaseUrlInvalid);
        }
        if parsed.query().is_some() || parsed.fragment().is_some() {
            return Err(NetworkPolicyRejection::BaseUrlInvalid);
        }

        let host = parsed
            .host()
            .ok_or(NetworkPolicyRejection::BaseUrlInvalid)?;
        let (host_text, is_ip_literal) = match host {
            url::Host::Domain(name) => (name.to_owned(), false),
            url::Host::Ipv4(address) => (address.to_string(), true),
            url::Host::Ipv6(address) => (address.to_string(), true),
        };
        if host_text.is_empty() {
            return Err(NetworkPolicyRejection::BaseUrlInvalid);
        }

        let text_loopback = !is_ip_literal && host_text.eq_ignore_ascii_case(LOCALHOST_NAME);
        let literal_loopback = is_ip_literal
            && host_text
                .parse::<IpAddr>()
                .map(is_loopback_address)
                .unwrap_or(false);
        let loopback = text_loopback || literal_loopback;

        if scheme == "http" && !loopback {
            return Err(NetworkPolicyRejection::NonLoopbackHttpForbidden);
        }
        if scheme == "https" && is_denied_literal(&host_text) {
            // https إلى عنوان وصفي/محلي محظور أيضاً: التشفير لا يبرّر الوصول إليه.
            return Err(NetworkPolicyRejection::DestinationDenied);
        }

        let port = parsed
            .port_or_known_default()
            .ok_or(NetworkPolicyRejection::BaseUrlInvalid)?;
        if port == 0 {
            return Err(NetworkPolicyRejection::BaseUrlInvalid);
        }

        // حلّ الاسم ثم رفض النتيجة إن خالفها أي عنوان واحد (فشل مغلق).
        let addresses = self.resolver.resolve(&host_text, port)?;
        if addresses.is_empty() {
            return Err(NetworkPolicyRejection::NoResolvedAddress);
        }
        for address in &addresses {
            if is_denied_address(*address) {
                return Err(NetworkPolicyRejection::DestinationDenied);
            }
        }
        // حلقة صريحة عبر اسم (localhost) تتطلب أن تكون كل العناوين حلقية.
        if loopback
            && !addresses
                .iter()
                .all(|address| is_loopback_address(*address))
        {
            return Err(NetworkPolicyRejection::DestinationDenied);
        }

        Ok(AllowedDestination {
            scheme: scheme.to_owned(),
            host: host_text,
            port,
            resolved_addresses: addresses,
            loopback,
            limits: self.limits.clone(),
        })
    }
}

/// فحص بنيوي لوجود userinfo في السلطة (authority) — يعمل على النص الخام قبل
/// التطبيع لأن التطبيع يُسقط الاعتماد الفارغ ويُخفي النية.
/// الصيغة المفحوصة: scheme "://" authority حيث تنتهي السلطة عند أول / أو ? أو #.
fn authority_carries_credentials(candidate: &str) -> bool {
    let Some(separator) = candidate.find("://") else {
        return false;
    };
    let remainder = &candidate[separator + 3..];
    let authority_end = remainder.find(['/', '?', '#']).unwrap_or(remainder.len());
    let authority = &remainder[..authority_end];
    // آخر '@' يفصل الاعتماد عن المضيف (اسم المستخدم قد يحمل '@' مُرمَّزاً فقط،
    // لكن آخر '@' هو الفاصل الحقيقي في صيغة WHATWG).
    authority.rfind('@').is_some()
}

/// هل العنوان حلقي؟ 127.0.0.0/8 (كلها، لا 127.0.0.1 وحدها) أو ::1.
pub fn is_loopback_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}

/// النطاقات المرفوضة: link-local، متعدد البث، غير محدد، خاص، وبيانات وصفية.
/// القائمة صريحة ومقصودة — كل إضافة هنا تضيّق السطح لا توسّعه.
pub fn is_denied_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => is_denied_v4(v4),
        IpAddr::V6(v6) => is_denied_v6(v6),
    }
}

fn is_denied_literal(host_text: &str) -> bool {
    host_text
        .parse::<IpAddr>()
        .map(is_denied_address)
        .unwrap_or(false)
}

fn is_denied_v4(address: Ipv4Addr) -> bool {
    let octets = address.octets();
    address.is_unspecified()                       // 0.0.0.0/32
        || address.is_broadcast()                  // 255.255.255.255/32
        || address.is_link_local()                 // 169.254.0.0/16 (يشمل 169.254.169.254)
        || address.is_multicast()                  // 224.0.0.0/4
        || address.is_private()                    // 10/8, 172.16/12, 192.168/16
        || address.is_documentation()              // 192.0.2.0/24 … (لا خدمة حقيقية)
        || octets[0] == 0                          // 0.0.0.0/8 «هذا المضيف»
        || (octets[0] == 100 && (64..128).contains(&octets[1])) // 100.64/10 CGNAT
        || (octets[0] == 198 && (octets[1] == 18 || octets[1] == 19)) // 198.18/15
        || octets[0] >= 240 // 240.0.0.0/4 محجوز + البث
}

fn is_denied_v6(address: Ipv6Addr) -> bool {
    let segments = address.segments();
    // fe80::/10 link-local.
    let link_local = (segments[0] & 0xffc0) == 0xfe80;
    // fc00::/7 unique-local.
    let unique_local = (segments[0] & 0xfe00) == 0xfc00;
    // ff00::/8 multicast.
    let multicast = (segments[0] & 0xff00) == 0xff00;
    // ::ffff:a.b.c.d — تُفحص كـ IPv4 لأن النية تُخفى في التغليف. عنوان IPv4
    // مُغلَّف إلى IPv6 يجب أن يخضع لقواعد IPv4 نفسها لا لتصنيف IPv6.
    let mapped_denied = address.to_ipv4_mapped().map(is_denied_v4).unwrap_or(false);

    // الحلقيات (::1) مسموحة صراحةً ولا تُدرج هنا؛ كل ما عدا ذلك من الفئات
    // الخاصة/المحجوزة مرفوض.
    address.is_unspecified() || link_local || unique_local || multicast || mapped_denied
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    use std::time::Duration;

    use super::{
        is_denied_address, AllowedDestination, HostResolver, NetworkPolicy, NetworkPolicyRejection,
        StaticResolver, TransportLimits,
    };

    fn ip(value: &str) -> IpAddr {
        value.parse().expect("عنوان الاختبار يجب أن يُحلَّل")
    }

    fn resolver_with(host: &str, addresses: &[&str]) -> StaticResolver {
        StaticResolver::new(vec![(
            host,
            addresses.iter().map(|value| ip(value)).collect(),
        )])
    }

    fn evaluate(
        base_url: &str,
        resolver: &StaticResolver,
    ) -> Result<AllowedDestination, NetworkPolicyRejection> {
        NetworkPolicy::new(resolver).evaluate(base_url)
    }

    // ===== قاعدة 1: المخططات المسموحة =====

    #[test]
    fn rejects_every_scheme_other_than_http_and_https() {
        let resolver = resolver_with("files.invalid", &["93.184.216.34"]);
        for candidate in [
            "file:///etc/passwd",
            "ftp://host.invalid/x",
            "data:text/plain,hello",
            "ws://host.invalid/socket",
            "gopher://host.invalid/",
        ] {
            assert_eq!(
                evaluate(candidate, &resolver),
                Err(NetworkPolicyRejection::SchemeNotAllowed),
                "يجب رفض المخطط في {candidate}"
            );
        }
    }

    #[test]
    fn rejects_urls_that_are_not_absolute_and_well_formed() {
        let resolver = resolver_with("api.example.com", &["93.184.216.34"]);
        for candidate in [
            "",
            "   ",
            "api.example.com",
            "//api.example.com/v1",
            "http://api.example.com/v1\nHost: evil.invalid",
            "http://api.example.com/v1 extra",
            "https://api.example.com/v1?query=1",
            "https://api.example.com/v1#fragment",
            "ftp://api.example.com",
        ] {
            assert!(
                evaluate(candidate, &resolver).is_err(),
                "يجب رفض المدخل غير الصالح {candidate}"
            );
        }
    }

    // ===== قاعدة 2: لا بيانات اعتماد داخل العنوان =====

    #[test]
    fn rejects_urls_carrying_userinfo_credentials() {
        let resolver = resolver_with("api.example.com", &["93.184.216.34"]);
        for candidate in [
            "https://user:secret@api.example.com",
            "https://user@api.example.com",
            "https://:@api.example.com",
            "http://user:secret@127.0.0.1:11434",
            "https://user:secret@127.0.0.1:11434",
        ] {
            assert_eq!(
                evaluate(candidate, &resolver),
                Err(NetworkPolicyRejection::CredentialsInUrl),
                "يجب رفض الاعتماد المضمّن في {candidate}"
            );
        }
    }

    // ===== قاعدة 3: https إلزامي لغير الحلقي، http للحلقي فقط =====

    #[test]
    fn rejects_plain_http_to_any_non_loopback_host() {
        let resolver = StaticResolver::new(vec![
            ("api.example.com", vec![ip("93.184.216.34")]),
            ("169.254.169.254", vec![ip("169.254.169.254")]),
        ]);
        assert_eq!(
            evaluate("http://api.example.com", &resolver),
            Err(NetworkPolicyRejection::NonLoopbackHttpForbidden)
        );
        // المسار يُرفض أولاً (الأساس يجب أن يكون أصلاً بلا مسار).
        assert_eq!(
            evaluate("http://169.254.169.254/latest/meta-data/", &resolver),
            Err(NetworkPolicyRejection::BaseUrlInvalid),
            "المسار مرفوض قبل الوصول إلى فحص الوجهة"
        );
        assert_eq!(
            evaluate("http://169.254.169.254", &resolver),
            Err(NetworkPolicyRejection::NonLoopbackHttpForbidden)
        );
    }

    #[test]
    fn accepts_plain_http_only_for_loopback_hosts() {
        let resolver = StaticResolver::new(vec![
            ("127.0.0.1", vec![ip("127.0.0.1")]),
            ("127.13.5.9", vec![ip("127.13.5.9")]),
            ("::1", vec![ip("::1")]),
            ("localhost", vec![ip("127.0.0.1"), ip("::1")]),
        ]);
        for (candidate, expected_host) in [
            ("http://127.0.0.1:11434", "127.0.0.1"),
            ("http://127.13.5.9:1234", "127.13.5.9"),
            ("http://[::1]:11434", "::1"),
            ("http://localhost:11434", "localhost"),
        ] {
            let allowed =
                evaluate(candidate, &resolver).unwrap_or_else(|_| panic!("يجب قبول {candidate}"));
            assert!(allowed.is_loopback(), "{candidate} يجب أن يُصنَّف حلقياً");
            assert!(!allowed.is_secure(), "{candidate} يبقى http");
            assert_eq!(allowed.host(), expected_host);
        }
    }

    // ===== قاعدة 4: النطاقات المحظورة =====

    #[test]
    fn rejects_resolved_link_local_multicast_unspecified_and_private_ranges() {
        for denied in [
            "0.0.0.0",
            "169.254.169.254",
            "169.254.0.1",
            "224.0.0.1",
            "239.255.255.250",
            "255.255.255.255",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "100.64.0.1",
            "198.18.0.1",
            "240.0.0.1",
            "::",
            "fe80::1",
            "fc00::1",
            "fd12:3456::1",
            "ff02::1",
            "::ffff:169.254.169.254",
        ] {
            assert!(
                is_denied_address(ip(denied)),
                "العنوان {denied} يجب أن يكون محظوراً"
            );
        }
        for allowed in [
            "93.184.216.34",
            "8.8.8.8",
            "1.1.1.1",
            "2606:4700::1111",
            // الحلقيات مسموحة صراحةً — ومنها المُغلَّفة ::ffff:127.0.0.1.
            "127.0.0.1",
            "::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(
                !is_denied_address(ip(allowed)),
                "العنوان {allowed} لا يجوز أن يكون محظوراً"
            );
        }
    }

    #[test]
    fn denies_destinations_whose_resolved_address_is_in_a_denied_range() {
        // مثال DNS rebinding: اسم عام مقبول نصّاً لكنه يُحلّ إلى العنوان الوصفي.
        let resolver = resolver_with("metadata.looks-public.example", &["169.254.169.254"]);
        assert_eq!(
            evaluate("https://metadata.looks-public.example", &resolver),
            Err(NetworkPolicyRejection::DestinationDenied)
        );

        let resolver = resolver_with("internal.example", &["10.0.0.5"]);
        assert_eq!(
            evaluate("https://internal.example", &resolver),
            Err(NetworkPolicyRejection::DestinationDenied)
        );

        // حلّ يعطي عنواناً محظوراً إلى جانب عنوان عام ⇒ رفض كامل (فشل مغلق).
        // العنوان «الحلقي» ليس محظوراً بمفرده، لذا نستعمل نطاقاً خاصاً هنا
        // حتى يقيس الاختبار قاعدة الحظر الفعلية.
        let resolver = resolver_with("mixed.example", &["93.184.216.34", "10.0.0.5"]);
        assert_eq!(
            evaluate("https://mixed.example", &resolver),
            Err(NetworkPolicyRejection::DestinationDenied)
        );

        // حلّ يعطي عنواناً حلقياً مع عام: مسموح، لأن 127.0.0.0/8 ليست نطاقاً محظوراً
        // بل نطاق نقل غير مشفّر لا يُسمح به إلا لنفسه. التثبيت يحفظ العنوانين معاً
        // ولا يُعاد الحل بعد التحقق (هذا هو حدّ الدفاع من إعادة ربط DNS).
        let resolver = resolver_with("mixed.example", &["93.184.216.34", "127.0.0.1"]);
        let allowed = evaluate("https://mixed.example", &resolver).expect("يجب القبول");
        assert_eq!(
            allowed.resolved_addresses(),
            [ip("93.184.216.34"), ip("127.0.0.1")]
        );
        assert!(!allowed.is_loopback());
        assert!(allowed.is_secure());
    }

    #[test]
    fn denies_cloud_metadata_ip_literals_over_https_too() {
        let resolver = resolver_with("169.254.169.254", &["169.254.169.254"]);
        assert_eq!(
            evaluate("https://169.254.169.254", &resolver),
            Err(NetworkPolicyRejection::DestinationDenied)
        );
        let resolver = resolver_with("0.0.0.0", &["0.0.0.0"]);
        assert_eq!(
            evaluate("https://0.0.0.0", &resolver),
            Err(NetworkPolicyRejection::DestinationDenied)
        );
    }

    #[test]
    fn rejects_localhost_when_it_resolves_outside_loopback() {
        let resolver = resolver_with("localhost", &["10.0.0.7"]);
        assert_eq!(
            evaluate("http://localhost:11434", &resolver),
            Err(NetworkPolicyRejection::DestinationDenied)
        );
    }

    #[test]
    fn fails_closed_when_the_host_cannot_be_resolved() {
        let resolver = StaticResolver::new(vec![]);
        assert_eq!(
            evaluate("https://nowhere.invalid", &resolver),
            Err(NetworkPolicyRejection::HostUnresolved)
        );
    }

    // ===== قاعدة 5: قيود النقل المفروضة =====

    #[test]
    fn imposes_zero_redirects_no_proxy_and_bounded_timeouts() {
        let resolver = resolver_with("api.example.com", &["93.184.216.34"]);
        let allowed = evaluate("https://api.example.com", &resolver).expect("يجب القبول");
        let limits = allowed.limits();
        assert_eq!(limits.max_redirects, 0);
        assert!(!limits.allow_proxy);
        assert!(limits.connect_timeout <= Duration::from_secs(10));
        assert!(limits.global_timeout >= limits.connect_timeout);
        assert!(limits.max_response_bytes > 0);
        assert_eq!(limits.max_response_bytes, super::DEFAULT_MAX_RESPONSE_BYTES);

        let custom = TransportLimits {
            max_redirects: 0,
            connect_timeout: Duration::from_secs(1),
            global_timeout: Duration::from_secs(2),
            max_response_bytes: 4096,
            allow_proxy: false,
        };
        let policy = NetworkPolicy::with_limits(&resolver, custom.clone());
        let allowed = policy
            .evaluate("https://api.example.com")
            .expect("يجب القبول");
        assert_eq!(allowed.limits(), &custom);
    }

    // ===== رموز الرفض: ثابتة ولا تسرّب أي تفصيل =====

    #[test]
    fn rejection_codes_are_stable_and_leak_no_address_detail() {
        let cases = [
            (
                NetworkPolicyRejection::BaseUrlInvalid,
                "NETWORK_BASE_URL_INVALID",
            ),
            (
                NetworkPolicyRejection::SchemeNotAllowed,
                "NETWORK_SCHEME_NOT_ALLOWED",
            ),
            (
                NetworkPolicyRejection::CredentialsInUrl,
                "NETWORK_CREDENTIALS_IN_URL",
            ),
            (
                NetworkPolicyRejection::NonLoopbackHttpForbidden,
                "NETWORK_NON_LOOPBACK_HTTP_FORBIDDEN",
            ),
            (
                NetworkPolicyRejection::DestinationDenied,
                "NETWORK_DESTINATION_DENIED",
            ),
            (
                NetworkPolicyRejection::HostUnresolved,
                "NETWORK_HOST_UNRESOLVED",
            ),
            (
                NetworkPolicyRejection::NoResolvedAddress,
                "NETWORK_NO_RESOLVED_ADDRESS",
            ),
        ];
        for (rejection, expected) in cases {
            assert_eq!(rejection.code(), expected);
            // الرمز لا يحمل نقطة ولا شرطة مائلة ولا رقماً — أي أثر لعنوان أو منفذ.
            assert!(!expected.contains('.'));
            assert!(!expected.contains('/'));
            assert!(!expected.contains(':'));
            assert!(expected
                .chars()
                .all(|character| character.is_ascii_uppercase() || character == '_'));
        }
    }

    #[test]
    fn allowed_destination_never_exposes_credentials_in_its_origin() {
        let resolver = resolver_with("api.example.com", &["93.184.216.34"]);
        // منفذ غير افتراضي يُذكر صراحةً.
        let allowed = evaluate("https://api.example.com:8443", &resolver).expect("يجب القبول");
        assert_eq!(allowed.origin(), "https://api.example.com:8443");
        assert_eq!(allowed.port(), 8443);
        assert!(allowed.is_secure());
        assert!(!allowed.origin().contains('@'));
        assert_eq!(allowed.resolved_addresses(), [ip("93.184.216.34")]);

        // المنفذ الافتراضي لا يُكرر في الأصل المعروض.
        let default_https = evaluate("https://api.example.com", &resolver).expect("يجب القبول");
        assert_eq!(default_https.origin(), "https://api.example.com");
        assert_eq!(default_https.port(), 443);

        let loopback_resolver = resolver_with("127.0.0.1", &["127.0.0.1"]);
        let loopback = evaluate("http://127.0.0.1:11434", &loopback_resolver).expect("يجب القبول");
        assert_eq!(loopback.origin(), "http://127.0.0.1:11434");
        assert_eq!(loopback.port(), 11434);
    }

    #[test]
    fn system_resolver_is_the_production_default_and_resolves_localhost() {
        let resolved = super::SystemResolver
            .resolve("localhost", 11434)
            .expect("حلّ localhost يجب أن ينجح على أي جهاز");
        assert!(!resolved.is_empty());
        assert!(resolved
            .iter()
            .all(|address| super::is_loopback_address(*address)));
    }

    #[test]
    fn ipv6_literal_loopback_is_denied_only_when_not_loopback() {
        assert!(super::is_loopback_address(IpAddr::V6(Ipv6Addr::LOCALHOST)));
        assert!(!is_denied_address(IpAddr::V6(Ipv6Addr::LOCALHOST)));
        assert!(super::is_loopback_address(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        assert!(!is_denied_address(IpAddr::V4(Ipv4Addr::LOCALHOST)));
    }
}
