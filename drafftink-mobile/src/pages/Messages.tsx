import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import Layout from '../components/Layout';
import { listMessages } from '../api/client';
import { decryptSm4Text } from '../crypto/sm4';
import type { MessageView } from '../api/types';
import { formatDateTime } from '../utils/labels';

interface DecryptedMsg extends MessageView {
  body: string;
}

export default function Messages() {
  const nav = useNavigate();
  const [msgs, setMsgs] = useState<DecryptedMsg[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    listMessages()
      .then(async (list) => {
        const decrypted = await Promise.all(
          list.map(async (m) => ({ ...m, body: await decryptSm4Text(m.encrypted_body) })),
        );
        setMsgs(decrypted);
      })
      .catch(() => setMsgs([]))
      .finally(() => setLoading(false));
  }, []);

  return (
    <Layout title="消息中心" subtitle="SM4 信封加密 · 本地解密">
      <div className="hint">
        消息正文经 SM4（GB/T 32907-2016，ECB + PKCS#7）加密传输，密钥由设备指纹与校内共享密钥
        在本地派生，明文不出设备。
      </div>
      {loading && <div className="empty">加载中…</div>}
      {!loading && msgs.length === 0 && <div className="empty">暂无消息</div>}
      {msgs.map((m) => (
        <div className="list-item" key={m.id}>
          <div className="row" style={{ justifyContent: 'space-between' }}>
            <div className="title">{m.title}</div>
            <span className={`badge ${m.read ? 'gray' : 'blue'}`}>{m.read ? '已读' : '未读'}</span>
          </div>
          <div className="meta" style={{ marginTop: 6 }}>
            {m.channel} · {formatDateTime(m.created_at)}
          </div>
          <div style={{ marginTop: 8, fontSize: 14, lineHeight: 1.6 }}>{m.body}</div>
        </div>
      ))}
      <div style={{ height: 10 }} />
      <button className="btn ghost" onClick={() => nav('/')}>
        返回工作台
      </button>
    </Layout>
  );
}
