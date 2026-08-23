import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import Layout from '../components/Layout';
import { listAnnouncements } from '../api/client';
import type { Announcement } from '../api/types';

export default function Announcements() {
  const nav = useNavigate();
  const [list, setList] = useState<Announcement[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    listAnnouncements()
      .then((l) => setList(l.sort((a, b) => Number(b.pinned) - Number(a.pinned))))
      .catch(() => setList([]))
      .finally(() => setLoading(false));
  }, []);

  return (
    <Layout title="通知公告" subtitle="ZXBG0201">
      {loading && <div className="empty">加载中…</div>}
      {!loading && list.length === 0 && <div className="empty">暂无通知公告</div>}
      {list.map((a) => (
        <div className="list-item" key={a.notice_id}>
          <div className="row" style={{ justifyContent: 'space-between' }}>
            <div className="title">
              {a.pinned && <span className="badge blue" style={{ marginRight: 6 }}>置顶</span>}
              {a.title}
            </div>
          </div>
          <div className="meta" style={{ marginTop: 6 }}>
            {a.publisher} · {a.publish_date} · 接收：{a.recv_scope}
          </div>
          <div style={{ marginTop: 8, fontSize: 14, lineHeight: 1.6 }}>{a.body}</div>
        </div>
      ))}
      <div style={{ height: 10 }} />
      <button className="btn ghost" onClick={() => nav('/')}>
        返回工作台
      </button>
    </Layout>
  );
}
