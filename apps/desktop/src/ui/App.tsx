import { useState } from 'react';
import { Archive, AtSign, Download, Inbox, Plus, Search, Send, Settings, Star } from 'lucide-react';
import { check } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

const chats = [
  { name: 'Alex Johnson', service: 'WhatsApp · Personal', preview: 'Sounds good! See you then 👋', unread: 2 },
  { name: 'Product Team', service: 'Slack · Work', preview: 'Emma: New build is live!', unread: 5 },
  { name: 'Design', service: 'Discord · Personal', preview: "You: Here's the latest mockup", unread: 0 },
  { name: 'Family', service: 'Telegram · Personal', preview: "Don't forget dinner tonight ❤️", unread: 1 },
  { name: 'Lucas', service: 'Instagram · Personal', preview: 'Reacted to your story', unread: 0 }
];

const services = ['WhatsApp', 'Telegram', 'Discord', 'Instagram', 'Signal', 'Slack'];

export function App() {
  const [selected, setSelected] = useState(1);
  const [updating, setUpdating] = useState(false);
  const [updateMessage, setUpdateMessage] = useState('Check for updates');

  async function runUpdate() {
    setUpdating(true);
    setUpdateMessage('Checking…');
    try {
      const update = await check();
      if (!update) {
        setUpdateMessage('You are up to date');
        return;
      }
      setUpdateMessage(`Installing ${update.version}…`);
      await update.downloadAndInstall();
      await relaunch();
    } catch (error) {
      console.error(error);
      setUpdateMessage('Update check failed');
    } finally {
      setUpdating(false);
    }
  }

  return <div className="shell">
    <aside className="sidebar">
      <div className="brand"><div className="brandMark">U</div><div><strong>Unibox</strong><span>All your chats. One box.</span></div></div>
      <nav>
        <button className="active"><Inbox size={17}/>All Chats <b>12</b></button>
        <button><Star size={17}/>Unread</button>
        <button><AtSign size={17}/>Mentions</button>
        <button><Archive size={17}/>Archive</button>
      </nav>
      <div className="sectionTitle">Connected services</div>
      {services.map(service => <button className="service" key={service}><span className="serviceDot"/>{service}<i/></button>)}
      <button className="add"><Plus size={17}/>Add service</button>
      <div className="sidebarBottom">
        <button disabled={updating} onClick={runUpdate}><Download size={16}/>{updateMessage}</button>
        <button><Settings size={16}/>Settings</button>
      </div>
    </aside>
    <section className="listPane">
      <div className="search"><Search size={17}/><input aria-label="Search" placeholder="Search messages, people or links…"/><kbd>Ctrl K</kbd></div>
      <div className="filters"><span className="selected">All</span><span>Direct</span><span>Groups</span><span>Unread</span><span>Favorites</span></div>
      <div className="chatList">{chats.map((chat, index) => <button key={chat.name} onClick={() => setSelected(index)} className={selected === index ? 'chat activeChat' : 'chat'}>
        <div className="avatar">{chat.name[0]}</div>
        <div className="chatMeta"><strong>{chat.name}</strong><small>{chat.service}</small><span>{chat.preview}</span></div>
        {chat.unread > 0 && <b className="badge">{chat.unread}</b>}
      </button>)}</div>
    </section>
    <main className="conversation">
      <header><div><h2>{chats[selected].name}</h2><span>{chats[selected].service}</span></div><div className="headerActions"><button><Search size={18}/></button><button><Settings size={18}/></button></div></header>
      <div className="messages">
        <div className="bubble"><b>Emma</b><p>Hey team! The new build is live on all platforms.</p><small>09:14</small></div>
        <div className="bubble mine"><b>You</b><p>Tested locally on Windows. Everything looks smooth.</p><small>09:20</small></div>
        <div className="bubble"><b>Emma</b><p>Great. Auto-update and onboarding are next.</p><small>10:12</small></div>
      </div>
      <div className="composer"><button aria-label="Add attachment"><Plus size={18}/></button><input placeholder={`Message ${chats[selected].name}…`}/><button className="send" aria-label="Send"><Send size={18}/></button></div>
    </main>
  </div>;
}
